//! Capture drafts are local until one final, explicit Upload action.
use crate::common::{self, Result};
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;
const MAX_FILE: usize = 128 * 1024 * 1024;
const API: &str = "https://api.linear.app/graphql";
const FAILURE: &str = "Linear request failed. Your local capture is unchanged; review Linear before retrying a submission.";
fn valid_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok()
}
fn safe_link(value: &str, storage: bool) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && if storage {
                url.host_str() == Some("uploads.linear.app")
            } else {
                url.host_str() == Some("linear.app")
            }
    })
}
fn upload_link(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && matches!(
                url.host_str(),
                Some("storage.googleapis.com" | "uploads.linear.app")
            )
    })
}
fn capture(path: &Path) -> Result<(Vec<u8>, &'static str)> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| "Choose an accessible local PNG or MP4 capture.")?;
    let before = file.metadata().map_err(|_| "Capture unavailable.")?;
    if !before.is_file()
        || before.uid() != unsafe { libc::geteuid() }
        || before.nlink() != 1
        || before.len() == 0
        || before.len() > MAX_FILE as u64
    {
        return Err("Capture must be your regular PNG or MP4 file, at most 128 MiB.");
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Capture could not be read.")?;
    let after = file.metadata().map_err(|_| "Capture unavailable.")?;
    if bytes.len() != before.len() as usize
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
    {
        return Err("Capture changed while reading; choose it again.");
    }
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.get(4..8) == Some(b"ftyp") {
        "video/mp4"
    } else {
        return Err("Only PNG screenshots and MP4 recordings are supported.");
    };
    Ok((bytes, mime))
}
async fn dialog(
    args: Vec<String>,
    input: Vec<u8>,
    cancel: CancellationToken,
) -> Result<Option<String>> {
    let mut cmd = Command::new("zenity");
    cmd.args(args);
    match common::command(cmd, input, Duration::from_secs(900), 128 * 1024, cancel).await {
        Ok(bytes) => String::from_utf8(bytes)
            .map(|s| Some(s.trim_end_matches('\n').to_owned()))
            .map_err(|_| "Invalid dialog response."),
        Err(_) => Ok(None),
    }
}
async fn wallet(
    action: &str,
    input: Vec<u8>,
    cancel: CancellationToken,
) -> Result<Zeroizing<String>> {
    let mut cmd = Command::new("secret-tool");
    cmd.arg(action);
    if action == "store" {
        cmd.arg("--label=Seele Linear capture");
    }
    cmd.args(["application", "seele-linear-capture", "account", "linear"]);
    let bytes = common::command(cmd, input, Duration::from_secs(120), 8192, cancel)
        .await
        .map_err(|_| "Unlock your system wallet and connect Linear, then retry.")?;
    let token = Zeroizing::new(
        String::from_utf8(bytes)
            .map_err(|_| "Invalid wallet response.")?
            .trim_end_matches('\n')
            .to_owned(),
    );
    if action == "lookup"
        && (token.len() < 8 || token.len() > 4096 || !token.bytes().all(|b| b.is_ascii_graphic()))
    {
        return Err("Connect Linear with a personal API key first.");
    }
    Ok(token)
}
struct Api {
    http: reqwest::Client,
    endpoint: String,
    token: Zeroizing<String>,
}
impl Api {
    fn new(token: Zeroizing<String>) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(120))
                .connect_timeout(Duration::from_secs(10))
                .no_proxy()
                .build()
                .map_err(|_| FAILURE)?,
            endpoint: API.into(),
            token,
        })
    }
    async fn query(&self, query: &str, variables: Value) -> Result<Value> {
        let response = self
            .http
            .post(&self.endpoint)
            .header("Authorization", self.token.as_str())
            .json(&json!({"query":query,"variables":variables}))
            .send()
            .await
            .map_err(|_| FAILURE)?;
        if !response.status().is_success() {
            return Err(FAILURE);
        }
        let body = common::response_json(response).await.map_err(|_| FAILURE)?;
        if body.get("errors").is_some() || !body["data"].is_object() {
            return Err(FAILURE);
        }
        Ok(body["data"].clone())
    }
    fn storage_link(&self, value: &str, upload: bool) -> bool {
        #[cfg(test)]
        if let (Ok(endpoint), Ok(url)) = (url::Url::parse(&self.endpoint), url::Url::parse(value)) {
            if endpoint.scheme() == "http"
                && endpoint.host_str() == Some("127.0.0.1")
                && endpoint.origin() == url.origin()
            {
                return true;
            }
        }
        if upload {
            upload_link(value)
        } else {
            safe_link(value, true)
        }
    }
    async fn upload(&self, bytes: Vec<u8>, mime: &str) -> Result<String> {
        let filename = if mime == "image/png" {
            "capture.png"
        } else {
            "recording.mp4"
        };
        let body=self.query("mutation Upload($type:String!,$name:String!,$size:Int!){fileUpload(contentType:$type,filename:$name,size:$size){success uploadFile{uploadUrl assetUrl headers{key value}}}}",json!({"type":mime,"name":filename,"size":bytes.len()})).await?;
        let payload = &body["fileUpload"];
        let file = &payload["uploadFile"];
        let upload = file["uploadUrl"]
            .as_str()
            .filter(|s| self.storage_link(s, true))
            .ok_or(FAILURE)?;
        let asset = file["assetUrl"]
            .as_str()
            .filter(|s| self.storage_link(s, false))
            .ok_or(FAILURE)?;
        if payload["success"] != true {
            return Err(FAILURE);
        }
        // A separate request has no API Authorization header. Never follow a
        // redirect or forward credentials to the signed storage endpoint.
        let mut request = self
            .http
            .put(upload)
            .header("Content-Type", mime)
            .header("Cache-Control", "public, max-age=31536000");
        let headers = file["headers"]
            .as_array()
            .filter(|h| h.len() <= 32)
            .ok_or(FAILURE)?;
        for header in headers {
            let key = header["key"].as_str().ok_or(FAILURE)?;
            let value = header["value"]
                .as_str()
                .filter(|v| v.len() <= 8192)
                .ok_or(FAILURE)?;
            if key.eq_ignore_ascii_case("authorization")
                || key.eq_ignore_ascii_case("cookie")
                || key.eq_ignore_ascii_case("host")
                || value.contains(self.token.as_str())
            {
                return Err(FAILURE);
            }
            let key =
                reqwest::header::HeaderName::from_bytes(key.as_bytes()).map_err(|_| FAILURE)?;
            let value = reqwest::header::HeaderValue::from_str(value).map_err(|_| FAILURE)?;
            request = request.header(key, value);
        }
        let response = request.body(bytes).send().await.map_err(|_| FAILURE)?;
        if !response.status().is_success() {
            return Err(FAILURE);
        }
        Ok(asset.to_owned())
    }
}
fn clean(value: &Value) -> String {
    common::clean(value, "", 180)
}
async fn select(
    title: &str,
    rows: &[Value],
    caption: &str,
    cancel: CancellationToken,
) -> Result<Option<Value>> {
    if rows.is_empty() || rows.len() > 100 {
        return Err("No matching Linear choices are available.");
    }
    let mut args = vec![
        "--list".into(),
        format!("--title={title}"),
        "--column=Choice".into(),
        "--column=Name".into(),
        "--print-column=1".into(),
    ];
    for (index, row) in rows.iter().enumerate() {
        args.push(index.to_string());
        args.push(clean(&row[caption]));
    }
    let Some(choice) = dialog(args, vec![], cancel).await? else {
        return Ok(None);
    };
    rows.get(choice.parse::<usize>().map_err(|_| "Invalid selection.")?)
        .cloned()
        .map(Some)
        .ok_or("Invalid selection.")
}
pub async fn run(args: Vec<String>, cancel: CancellationToken) -> Result<()> {
    if args.as_slice() == ["connect"] {
        let Some(key) = dialog(
            vec![
                "--password".into(),
                "--title=Connect Linear".into(),
                "--text=Enter a Linear personal API key. It will be stored in your system wallet."
                    .into(),
            ],
            vec![],
            cancel.clone(),
        )
        .await?
        else {
            return Ok(());
        };
        let key = Zeroizing::new(key);
        if key.len() < 8 || key.len() > 4096 || !key.bytes().all(|b| b.is_ascii_graphic()) {
            return Err("Invalid API key.");
        }
        wallet("store", key.as_bytes().to_vec(), cancel).await?;
        return Ok(());
    }
    if args.as_slice() == ["disconnect"] {
        wallet("clear", vec![], cancel).await?;
        return Ok(());
    }
    if args.len() > 1 {
        return Err("Usage: seele-linear-capture [capture-path|connect|disconnect]");
    }
    let path = if let Some(path) = args.first() {
        PathBuf::from(path)
    } else {
        let Some(path) = dialog(
            vec![
                "--file-selection".into(),
                "--title=Choose a capture for Linear".into(),
                "--file-filter=Captures | *.png *.mp4".into(),
            ],
            vec![],
            cancel.clone(),
        )
        .await?
        else {
            return Ok(());
        };
        PathBuf::from(path)
    };
    let (bytes, mime) = capture(&path)?;
    let Some(fields)=dialog(vec!["--forms".into(),"--title=Linear capture draft".into(),"--text=Create a new Seele issue or attach a comment to an existing issue. Nothing uploads before final review.".into(),"--add-entry=Existing issue ID (blank creates a new issue)".into(),"--add-entry=Title".into(),"--add-entry=Description".into(),"--separator=|".into()],vec![],cancel.clone()).await? else{return Ok(())};
    let mut fields = fields.splitn(3, '|');
    let target = fields.next().unwrap_or("").trim();
    let title = fields.next().unwrap_or("").trim();
    let description = fields.next().ok_or("Invalid draft.")?;
    if title.is_empty()
        || title.len() > 240
        || description.len() > 32 * 1024
        || target.len() > 64
        || !target
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-".contains(&b))
    {
        return Err("Enter a title and a valid issue ID, or leave the ID blank.");
    }
    // Only selected details are collected. No hostname, account, window title,
    // machine ID, logs or environment is included.
    let Some(choices) = dialog(
        vec![
            "--list".into(),
            "--checklist".into(),
            "--title=Optional system details".into(),
            "--text=Select only details to include. Both are off initially.".into(),
            "--column=Include".into(),
            "--column=Detail".into(),
            "--print-column=2".into(),
            "FALSE".into(),
            "Kernel version".into(),
            "FALSE".into(),
            "Architecture".into(),
            "--separator=|".into(),
        ],
        vec![],
        cancel.clone(),
    )
    .await?
    else {
        return Ok(());
    };
    let mut text = description.to_owned();
    for choice in choices.split('|').filter(|s| !s.is_empty()) {
        let flag = match choice {
            "Kernel version" => "-r",
            "Architecture" => "-m",
            _ => return Err("Invalid metadata choice."),
        };
        let mut cmd = Command::new("uname");
        cmd.arg(flag);
        let result =
            common::command(cmd, vec![], Duration::from_secs(5), 1024, cancel.clone()).await?;
        let value = clean(&Value::String(
            String::from_utf8_lossy(&result).into_owned(),
        ));
        text.push_str(&format!("\n\n{choice}: {value}"));
    }
    let api = Api::new(wallet("lookup", vec![], cancel.clone()).await?)?;
    let (issue, project, team, summary) = if target.is_empty() {
        let data=api.query("query Projects{projects(first:100,filter:{name:{eq:\"Seele\"}}){nodes{id name teams(first:100){nodes{id name}}}}}",json!({})).await?;
        let rows = data["projects"]["nodes"].as_array().ok_or(FAILURE)?;
        let Some(project) = select("Seele project", rows, "name", cancel.clone()).await? else {
            return Ok(());
        };
        if project["name"] != "Seele" {
            return Err("Choose the Seele project.");
        }
        let teams = project["teams"]["nodes"].as_array().ok_or(FAILURE)?;
        let Some(team) = select("Issue team", teams, "name", cancel.clone()).await? else {
            return Ok(());
        };
        let project_id = project["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or(FAILURE)?
            .to_owned();
        let team_id = team["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or(FAILURE)?
            .to_owned();
        (
            None,
            Some(project_id),
            Some(team_id),
            format!(
                "Create issue in {} / {}",
                clean(&project["name"]),
                clean(&team["name"])
            ),
        )
    } else {
        let data = api
            .query(
                "query Issue($id:String!){issue(id:$id){id identifier title project{name}}}",
                json!({"id":target}),
            )
            .await?;
        let row = &data["issue"];
        if row["project"]["name"] != "Seele" {
            return Err("Choose an existing issue in the Seele project.");
        }
        let id = row["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or(FAILURE)?
            .to_owned();
        (
            Some(id),
            None,
            None,
            format!(
                "Attach comment to {}: {}",
                clean(&row["identifier"]),
                clean(&row["title"])
            ),
        )
    };
    let review=format!("{summary}\n\nTitle: {title}\n\n{text}\n\nAttachment: {} ({}, {} bytes)\n\nUpload sends this capture and the text above to Linear. No other system details are included.",clean(&json!(path.file_name().and_then(|s|s.to_str()).unwrap_or("capture"))),mime,bytes.len());
    // Feed review content through stdin, never command arguments or persistent files.
    if dialog(
        vec![
            "--text-info".into(),
            "--title=Review Linear capture".into(),
            "--width=640".into(),
            "--height=520".into(),
            "--ok-label=Upload to Linear".into(),
            "--cancel-label=Cancel".into(),
        ],
        review.into_bytes(),
        cancel.clone(),
    )
    .await?
    .is_none()
    {
        return Ok(());
    }
    let asset = api.upload(bytes, mime).await?;
    let body = format!("{text}\n\n![Capture]({asset})");
    let result = if let Some(id) = issue {
        api.query("mutation Comment($input:CommentCreateInput!){commentCreate(input:$input){success comment{url}}}",json!({"input":{"issueId":id,"body":format!("{title}\n\n{body}")}})).await?
    } else {
        api.query("mutation Create($input:IssueCreateInput!){issueCreate(input:$input){success issue{url}}}",json!({"input":{"title":title,"description":body,"projectId":project,"teamId":team}})).await?
    };
    let result = result
        .get("issueCreate")
        .or_else(|| result.get("commentCreate"))
        .ok_or(FAILURE)?;
    let link = result["issue"]["url"]
        .as_str()
        .or_else(|| result["comment"]["url"].as_str())
        .filter(|s| safe_link(s, false))
        .ok_or(FAILURE)?;
    if result["success"] != true {
        return Err(FAILURE);
    }
    let mut cmd = Command::new("wl-copy");
    cmd.args(["--type", "text/plain"]);
    common::handoff(
        cmd,
        link.as_bytes().to_vec(),
        Duration::from_secs(10),
        cancel,
    )
    .await
    .map_err(|_| {
        "Submitted to Linear, but copying its link failed. Check Linear before retrying."
    })?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destinations() {
        assert!(upload_link(
            "https://storage.googleapis.com/linear/file?signature=abc"
        ));
        for link in [
            "http://storage.googleapis.com/file",
            "https://localhost/file",
            "https://storage.googleapis.com.evil/file",
            "https://user@storage.googleapis.com/file",
            "https://storage.googleapis.com:444/file",
        ] {
            assert!(!upload_link(link));
        }
        assert!(safe_link("https://uploads.linear.app/a/b", true));
        assert!(!safe_link("https://linear.app.evil/a", false));
    }
    #[test]
    fn bounds_and_magic() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("capture");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\nfixture").unwrap();
        assert_eq!(capture(&path).unwrap().1, "image/png");
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(capture(&link).is_err());
        std::fs::write(&path, b"unrelated private bytes").unwrap();
        assert!(capture(&path).is_err());
    }
}
#[cfg(test)]
mod http_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn server(
        replies: Vec<Value>,
    ) -> (String, tokio::task::JoinHandle<Vec<(String, Vec<u8>)>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let endpoint = format!("http://{addr}/graphql");
        let task = tokio::spawn(async move {
            let mut requests = vec![];
            for body in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = vec![];
                let mut buffer = [0; 4096];
                let (header_end, length) = loop {
                    let n = socket.read(&mut buffer).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    assert!(bytes.len() < 1024 * 1024);
                    if let Some(index) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&bytes[..index]);
                        let length = header
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|s| s.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        break (index + 4, length);
                    }
                };
                while bytes.len() < header_end + length {
                    let n = socket.read(&mut buffer).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                requests.push((
                    String::from_utf8(bytes[..header_end].to_vec()).unwrap(),
                    bytes[header_end..header_end + length].to_vec(),
                ));
                let body = body.to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            requests
        });
        (endpoint, task)
    }
    #[tokio::test]
    async fn upload_does_not_forward_api_key() {
        let (endpoint, task) = server_upload().await;
        let mut api = Api::new(Zeroizing::new("fake-linear-key".into())).unwrap();
        api.endpoint = endpoint.clone();
        let asset = api
            .upload(b"capture bytes".to_vec(), "image/png")
            .await
            .unwrap();
        assert_eq!(asset, endpoint.replace("/graphql", "/asset"));
        let requests = task.await.unwrap();
        assert!(requests[0]
            .0
            .to_ascii_lowercase()
            .contains("authorization: fake-linear-key"));
        assert!(!requests[1]
            .0
            .to_ascii_lowercase()
            .contains("authorization:"));
        assert_eq!(requests[1].1, b"capture bytes");
        let variables: Value = serde_json::from_slice(&requests[0].1).unwrap();
        assert_eq!(variables["variables"]["size"], 13);
    }
    async fn server_upload() -> (String, tokio::task::JoinHandle<Vec<(String, Vec<u8>)>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let endpoint = format!("http://{addr}/graphql");
        let task = tokio::spawn(async move {
            let mut result = vec![];
            for index in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = vec![];
                let mut buf = [0; 4096];
                loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&bytes[..end]);
                        let len = header
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|s| s.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            result.push((
                                header.into_owned(),
                                bytes[end + 4..end + 4 + len].to_vec(),
                            ));
                            break;
                        }
                    }
                }
                let body=if index==0{json!({"data":{"fileUpload":{"success":true,"uploadFile":{"uploadUrl":format!("http://{addr}/put"),"assetUrl":format!("http://{addr}/asset"),"headers":[{"key":"x-upload","value":"signed"}]}}}})}else{json!({})}.to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            result
        });
        (endpoint, task)
    }
    #[tokio::test]
    async fn graphql_errors_fail_even_with_data() {
        let (endpoint,task)=server(vec![json!({"data":{"issueCreate":{"success":true}},"errors":[{"message":"fake secret-looking remote error"}]})]).await;
        let mut api = Api::new(Zeroizing::new("fake-key".into())).unwrap();
        api.endpoint = endpoint;
        assert_eq!(
            api.query("mutation Fake", json!({})).await.unwrap_err(),
            FAILURE
        );
        task.await.unwrap();
    }
}
