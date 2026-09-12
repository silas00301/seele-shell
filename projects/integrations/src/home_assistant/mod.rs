mod connection;
mod live;
use crate::common::{self, Result, MAX_RESPONSE};
pub use live::watch;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs::OpenOptions,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
    process::Command,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;
pub const MAX_CONFIG: usize = 32768;
const INVALID_CONFIG: &str = "The connection file needs a valid URL, token and entity list.";

#[derive(Clone)]
pub struct Config {
    pub url: String,
    pub token: Zeroizing<String>,
    pub entities: Vec<Value>,
    pub summary: String,
    legacy: bool,
}
pub fn config_path() -> PathBuf {
    std::env::var_os("SEELE_HOME_ASSISTANT_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            common::xdg("XDG_CONFIG_HOME", ".config").join("seele-shell/home-assistant.json")
        })
}
pub fn valid_entity(s: &str) -> bool {
    let Some((domain, name)) = s.split_once('.') else {
        return false;
    };
    !domain.is_empty()
        && !name.is_empty()
        && s.len() <= 255
        && domain.bytes().all(|c| c.is_ascii_lowercase() || c == b'_')
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}
pub fn allowed(entity: &str) -> bool {
    matches!(
        entity.split('.').next(),
        Some("light" | "switch" | "input_boolean" | "fan")
    )
}
pub fn valid_url(value: &str) -> bool {
    if value.len() > 8192
        || value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return false;
    }
    url::Url::parse(value).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https")
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none()
    })
}
pub fn valid_token(token: &str) -> bool {
    !token.is_empty() && token.len() <= 4096 && token.bytes().all(|b| (33..=126).contains(&b))
}
fn selected(value: &Value, token: &str) -> Result<Vec<Value>> {
    let entries = value
        .as_array()
        .filter(|v| v.len() <= 32)
        .ok_or(INVALID_CONFIG)?;
    let mut seen = HashSet::new();
    entries.iter().map(|value| {
        let id = value.as_str().or_else(|| value["entity_id"].as_str()).ok_or(INVALID_CONFIG)?;
        if !valid_entity(id) || !seen.insert(id) || value.get("name").is_some_and(|v| !v.is_string()) { return Err(INVALID_CONFIG); }
        Ok(json!({"entity_id":id,"name":common::clean(&value["name"],token,120),"room":common::clean(&value["room"],token,120),"favorite":value["favorite"].as_bool().unwrap_or(false)}))
    }).collect()
}
pub async fn secret(
    action: &str,
    url: &str,
    token: Option<&str>,
    cancel: CancellationToken,
) -> Result<String> {
    let mut cmd = Command::new("secret-tool");
    cmd.arg(action);
    if action == "store" {
        cmd.arg("--label=Seele Home Assistant");
    }
    cmd.args([
        "application",
        "seele-home-assistant",
        "server",
        url.trim_end_matches('/'),
    ]);
    let bytes = common::command(
        cmd,
        token.unwrap_or("").as_bytes().to_vec(),
        Duration::from_secs(120),
        8192,
        cancel,
    )
    .await
    .map_err(|_| "Unlock your system keyring, then retry setup.")?;
    String::from_utf8(bytes)
        .map(|s| s.trim_end_matches('\n').to_owned())
        .map_err(|_| "The system keyring returned an invalid token.")
}
pub async fn load_config(cancel: CancellationToken) -> Result<Option<Config>> {
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(config_path())
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Could not open the private connection file."),
    };
    let info = file.metadata().map_err(|_| INVALID_CONFIG)?;
    if !info.is_file() || info.uid() != unsafe { libc::geteuid() } || info.mode() & 0o077 != 0 {
        return Err("The connection file must be owned by you with mode 0600.");
    }
    let mut bytes = Zeroizing::new(Vec::new());
    (&mut file)
        .take((MAX_CONFIG + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| INVALID_CONFIG)?;
    if bytes.len() > MAX_CONFIG {
        return Err("The connection file is too large.");
    }
    let mut value: Value = serde_json::from_slice(&bytes).map_err(|_| INVALID_CONFIG)?;
    let url = value["url"]
        .as_str()
        .filter(|u| valid_url(u))
        .ok_or(INVALID_CONFIG)?
        .trim_end_matches('/')
        .to_owned();
    let legacy = value.get("token").is_some();
    let token = if legacy {
        value["token"]
            .take()
            .as_str()
            .ok_or(INVALID_CONFIG)?
            .to_owned()
    } else {
        secret("lookup", &url, None, cancel).await?
    };
    let token = Zeroizing::new(token);
    if !valid_token(&token) {
        return Err(INVALID_CONFIG);
    }
    let entities = selected(&value["entities"], &token)?;
    let summary = value["summary"].as_str().unwrap_or("").to_owned();
    if !summary.is_empty() && !entities.iter().any(|v| v["entity_id"] == summary) {
        return Err(INVALID_CONFIG);
    }
    Ok(Some(Config {
        url,
        token,
        entities,
        summary,
        legacy,
    }))
}
pub fn save_config(config: &Config) -> Result<()> {
    let value = json!({"url":config.url,"entities":config.entities,"summary":config.summary});
    let bytes = serde_json::to_vec(&value).map_err(|_| INVALID_CONFIG)?;
    if bytes.len() > MAX_CONFIG {
        return Err("Too many display preferences to save.");
    }
    seele_runtime::fs::atomic_write(&config_path(), &bytes)
        .map_err(|_| "Could not save the private connection file.")
}
pub fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(4))
        .connect_timeout(Duration::from_secs(4))
        .build()
        .map_err(|_| "Home Assistant is unavailable.")
}
pub async fn request(config: &Config, path: &str, body: Option<Value>) -> Result<Value> {
    let client = client()?;
    let mut req = if body.is_some() {
        client.post(format!("{}{path}", config.url))
    } else {
        client.get(format!("{}{path}", config.url))
    };
    req = req
        .bearer_auth(config.token.as_str())
        .header("Accept", "application/json")
        .header("Content-Type", "application/json");
    if let Some(body) = body {
        req = req.json(&body);
    }
    let response = req
        .send()
        .await
        .map_err(|_| "Home Assistant is unreachable. Check the connection.")?;
    let status = response.status();
    if status.is_redirection() {
        return Err("The server redirected the request. Check the configured URL.");
    }
    if matches!(status.as_u16(), 401 | 403) {
        return Err("Access was denied. Check the Home Assistant token.");
    }
    if !status.is_success() {
        return Err("Home Assistant could not complete the request.");
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE as u64)
    {
        return Err("The server response is too large.");
    }
    common::response_json(response).await.map_err(|error| {
        if error == "response-too-large" {
            "The server response is too large."
        } else {
            "Home Assistant returned an invalid response."
        }
    })
}
pub async fn run(args: &[String], cancel: CancellationToken) -> Result<Value> {
    let config = load_config(cancel).await?;
    let status = args.is_empty() || args == ["status"];
    if !status {
        if args.len() != 3 || args[0] != "set" || !matches!(args[2].as_str(), "on" | "off") {
            return Err("Use status or set ENTITY on|off.");
        }
        let config = config
            .as_ref()
            .ok_or("Add a Home Assistant connection first.")?;
        let id = &args[1];
        if !config.entities.iter().any(|v| v["entity_id"] == *id) || !allowed(id) {
            return Err("This entity is read-only or is not selected.");
        }
        request(
            config,
            &format!(
                "/api/services/{}/turn_{}",
                id.split('.').next().unwrap(),
                args[2]
            ),
            Some(json!({"entity_id":id})),
        )
        .await?;
    }
    let Some(config) = config else {
        return Ok(json!({"configured":false,"connected":false,"entities":[],"error":""}));
    };
    let states = request(&config, "/api/states", None).await?;
    let states = states
        .as_array()
        .ok_or("Home Assistant returned an invalid state list.")?;
    let entries: Vec<Value> = config.entities.iter().map(|selected| {
        let id = selected["entity_id"].as_str().unwrap();
        let item = states.iter().find(|s| s["entity_id"] == id).unwrap_or(&Value::Null);
        let state = item["state"].as_str().unwrap_or("unavailable");
        let name = if selected["name"].as_str().is_some_and(|n| !n.is_empty()) { selected["name"].clone() } else { item["attributes"].get("friendly_name").cloned().unwrap_or(json!(id)) };
        json!({"entity_id":id,"name":common::clean(&name,&config.token,120),"state":common::clean(&json!(state),&config.token,120),"unit":common::clean(&item["attributes"]["unit_of_measurement"],&config.token,24),"available":!matches!(state,"unavailable"|"unknown"),"controllable":allowed(id)&&matches!(state,"on"|"off")})
    }).collect();
    Ok(json!({"configured":true,"connected":true,"entities":entries,"error":""}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_url_credentials_redirect_vectors_and_invalid_tokens() {
        for url in [
            "https://user:pass@host",
            "https://host?q=secret",
            "https://host/#secret",
            "ftp://host",
            "http://host:bad",
            "http://host\\evil",
            "http://host\n",
        ] {
            assert!(!valid_url(url));
        }
        assert!(valid_url("https://home.example:8123/base"));
        for token in ["", "x\ny", "é", "\u{7f}"] {
            assert!(!valid_token(token));
        }
    }
    #[test]
    fn entity_and_capability_allowlist() {
        for entity in ["light.desk", "sensor.x1", "input_boolean.awake"] {
            assert!(valid_entity(entity));
        }
        for entity in [
            "../lock.door",
            "light.x.y",
            "light.",
            "Light.desk",
            "light.desk/evil",
        ] {
            assert!(!valid_entity(entity));
        }
        assert!(!allowed("lock.door"));
        assert!(!allowed("alarm_control_panel.home"));
    }
}
