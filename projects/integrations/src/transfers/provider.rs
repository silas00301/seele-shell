//! Only this adapter knows the Tailscale LocalAPI and daemon progress fields.
use super::{files, Service};
use crate::common::{self, Result, MAX_RESPONSE};
use futures_util::TryStreamExt;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use tokio_util::{io::ReaderStream, sync::CancellationToken};
#[derive(Clone)]
pub struct Provider {
    client: reqwest::Client,
    events: reqwest::Client,
}
fn encoded(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}
impl Provider {
    pub fn new(path: PathBuf) -> Result<Self> {
        let builder = || {
            reqwest::Client::builder()
                .unix_socket(path.clone())
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .read_timeout(Duration::from_secs(60))
        };
        Ok(Self {
            client: builder().build().map_err(|_| "provider-unavailable")?,
            events: builder().build().map_err(|_| "provider-unavailable")?,
        })
    }
    fn url(route: &str) -> String {
        format!("http://local-tailscaled.sock/localapi/v0/{route}")
    }
    pub async fn request(&self, method: reqwest::Method, route: &str) -> Result<Value> {
        tokio::time::timeout(Duration::from_secs(30), async {
            let response = self
                .client
                .request(method, Self::url(route))
                .send()
                .await
                .map_err(|_| "provider-unavailable")?;
            if !matches!(response.status().as_u16(), 200 | 204) {
                return Err("provider-unavailable");
            }
            common::response_json(response).await
        })
        .await
        .map_err(|_| "provider-unavailable")?
    }
    pub async fn targets(&self) -> Result<Vec<Value>> {
        let status = self.request(reqwest::Method::GET, "status").await?;
        let owner = &status["Self"]["UserID"];
        if owner.is_null() {
            return Ok(vec![]);
        }
        let targets = self.request(reqwest::Method::GET, "file-targets").await?;
        Ok(targets
            .as_array()
            .into_iter()
            .flatten()
            .take(4096)
            .filter_map(|entry| {
                let node = &entry["Node"];
                let id = node["StableID"].as_str()?;
                if node["User"] != *owner
                    || node["Online"] != true
                    || id.is_empty()
                    || id.len() > 256
                {
                    return None;
                }
                let name = node["ComputedName"]
                    .as_str()
                    .or(node["Name"].as_str())
                    .unwrap_or("Personal device")
                    .trim_end_matches('.');
                Some(json!({"id":id,"name":common::clean(&json!(name),"",120)}))
            })
            .collect())
    }
    pub async fn waiting(&self) -> Result<Vec<Value>> {
        let list = self.request(reqwest::Method::GET, "files/").await?;
        let list = list.as_array().ok_or("invalid-response")?;
        list.iter()
            .take(4096)
            .map(|v| {
                let name = v["Name"].as_str().ok_or("invalid-filename")?;
                files::filename(name)?;
                let size = v["Size"].as_u64().ok_or("invalid-response")?;
                Ok(json!({"name":name,"size":size}))
            })
            .collect()
    }
    pub async fn forget(&self, name: &str) -> Result<()> {
        self.request(
            reqwest::Method::DELETE,
            &format!("files/{}", encoded(files::filename(name)?)),
        )
        .await?;
        Ok(())
    }
    pub async fn send(
        &self,
        target: &str,
        path: &Path,
        expected: u64,
        identity: Option<&files::Identity>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let source = files::open_source(path)?;
        let before = source.metadata().map_err(|_| "source-missing")?;
        if before.len() != expected || identity.is_some_and(|identity| !identity.matches(&before)) {
            return Err("source-changed");
        }
        let monitor = source.try_clone().map_err(|_| "source-missing")?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("invalid-filename")?;
        let stream = ReaderStream::with_capacity(tokio::fs::File::from_std(source), 256 * 1024);
        let work = async {
            let response = self
                .client
                .put(Self::url(&format!(
                    "file-put/{}/{}",
                    encoded(target),
                    encoded(name)
                )))
                .header(reqwest::header::CONTENT_LENGTH, expected)
                .body(reqwest::Body::wrap_stream(stream))
                .send()
                .await
                .map_err(|_| "interrupted")?;
            if response.status() != 200 {
                return Err("interrupted");
            }
            let _ = common::response_json(response).await; // Provider response is never retained or displayed.
            if !files::unchanged(&before, &monitor.metadata().map_err(|_| "source-changed")?) {
                return Err("source-changed");
            }
            Ok(())
        };
        tokio::select! {result=work=>result,_=cancel.cancelled()=>Err("cancelled")}
    }
    pub async fn receive(
        &self,
        name: &str,
        directory: &Path,
        cancel: &CancellationToken,
        service: &Arc<Service>,
        id: &str,
        index: usize,
    ) -> Result<(String, u64)> {
        let work = async {
            let mut response = self
                .client
                .get(Self::url(&format!(
                    "files/{}",
                    encoded(files::filename(name)?)
                )))
                .send()
                .await
                .map_err(|_| "receive-unavailable")?;
            if response.status() != 200 {
                return Err("receive-unavailable");
            }
            let expected = response.content_length().ok_or("receive-unavailable")?;
            let staged = files::Staged::new(directory)?;
            let mut output = tokio::fs::File::from_std(
                staged
                    .file
                    .try_clone()
                    .map_err(|_| "destination-unavailable")?,
            );
            let mut count = 0u64;
            while let Some(chunk) = response.chunk().await.map_err(|_| "interrupted")? {
                count = count
                    .checked_add(chunk.len() as u64)
                    .filter(|n| *n <= expected)
                    .ok_or("interrupted")?;
                output
                    .write_all(&chunk)
                    .await
                    .map_err(|_| "destination-unavailable")?;
                service.progress(id, index, count, Some(expected));
            }
            if count != expected {
                return Err("interrupted");
            }
            output
                .flush()
                .await
                .map_err(|_| "destination-unavailable")?;
            drop(output);
            let path = staged.publish(name)?;
            Ok((path.to_string_lossy().into_owned(), count))
        };
        tokio::select! {result=work=>result,_=cancel.cancelled()=>Err("cancelled")}
    }
    pub async fn events(&self, service: Arc<Service>, cancel: CancellationToken) {
        loop {
            let work = async {
                let response = self
                    .events
                    .get(Self::url("watch-ipn-bus?mask=64"))
                    .send()
                    .await
                    .map_err(|_| "provider-unavailable")?;
                if response.status() != 200 {
                    return Err("provider-unavailable");
                }
                let stream = response.bytes_stream().map_err(std::io::Error::other);
                let mut reader =
                    tokio::io::BufReader::new(tokio_util::io::StreamReader::new(stream));
                while let Some(line) = common::read_frame(&mut reader, MAX_RESPONSE)
                    .await
                    .map_err(|_| "invalid-response")?
                {
                    let value: Value =
                        serde_json::from_slice(&line).map_err(|_| "invalid-response")?;
                    service.event(&value);
                }
                Ok::<_, &'static str>(())
            };
            tokio::select! {_=work=>{},_=cancel.cancelled()=>break};
            tokio::select! {_=tokio::time::sleep(Duration::from_secs(2))=>{},_=cancel.cancelled()=>break};
        }
    }
}
