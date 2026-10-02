//! Stateless Streamable HTTP MCP, exposed only on this node's Tailscale IP.
//! Every request authenticates its real TCP peer through tailscaled; forwarded
//! headers, DNS lookalikes and approval operations cannot grant authority.
use super::{tools, Config, Shared, LIMIT};
use crate::common::{self, Result};
use serde_json::{json, Value};
use std::{
    net::{IpAddr, SocketAddr},
    process::Command,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;
fn peer_matches(value: &Value, wanted: &str) -> bool {
    let name = value["Node"]["Name"]
        .as_str()
        .unwrap_or("")
        .trim_end_matches('.');
    !name.is_empty()
        && if wanted.contains('.') {
            name.eq_ignore_ascii_case(wanted.trim_end_matches('.'))
        } else {
            name.split('.')
                .next()
                .is_some_and(|v| v.eq_ignore_ascii_case(wanted))
        }
}
async fn authorized(peer: SocketAddr, wanted: &str, cancel: CancellationToken) -> bool {
    let mut cmd = Command::new("tailscale");
    cmd.args(["whois", "--json", &peer.ip().to_string()]);
    common::command(cmd, vec![], Duration::from_secs(3), 65536, cancel)
        .await
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|v| peer_matches(&v, wanted))
}
async fn rpc(request: &Value, config: &Config, state: &Shared, cancel: CancellationToken) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let fail = |code: i32, message: &str| json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}});
    if request["jsonrpc"] != "2.0"
        || !request.is_object()
        || (!id.is_null() && !id.is_string() && !id.is_number())
    {
        return fail(-32600, "Invalid request");
    }
    let result = match request["method"].as_str() {
        Some("initialize") => Ok(
            json!({"protocolVersion":"2025-03-26","capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"seele-nerv","version":"1.0.0"}}),
        ),
        Some("ping") => Ok(json!({})),
        Some("tools/list") => Ok(json!({"tools":tools::catalog()})),
        Some("tools/call") => {
            let args = request["params"]
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = tools::call(
                request["params"]["name"].as_str().unwrap_or(""),
                &args,
                config,
                state,
                cancel,
            )
            .await;
            Ok(match result {
                Ok(v) => json!({"content":[{"type":"text","text":v.to_string()}],"isError":false}),
                Err(e) => json!({"content":[{"type":"text","text":e}],"isError":true}),
            })
        }
        _ => Err("Method not found"),
    };
    match result {
        Ok(v) => json!({"jsonrpc":"2.0","id":id,"result":v}),
        Err(e) => fail(-32601, e),
    }
}
async fn respond(stream: &mut TcpStream, code: &str, body: &[u8]) -> std::io::Result<()> {
    stream.write_all(format!("HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",body.len()).as_bytes()).await?;
    stream.write_all(body).await
}
fn headers(bytes: &[u8]) -> Result<(usize, usize)> {
    let end = bytes
        .windows(4)
        .position(|b| b == b"\r\n\r\n")
        .ok_or("Incomplete HTTP header.")?
        + 4;
    if end > 8192 {
        return Err("HTTP header exceeds its limit.");
    }
    let text = std::str::from_utf8(&bytes[..end]).map_err(|_| "Invalid HTTP header.")?;
    let mut lines = text.split("\r\n");
    if lines.next() != Some("POST /mcp HTTP/1.1") {
        return Err("Only POST /mcp is supported.");
    }
    let mut length = None;
    for line in lines.filter(|l| !l.is_empty()) {
        let (name, value) = line.split_once(':').ok_or("Invalid HTTP header.")?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&c))
        {
            return Err("Invalid HTTP header name.");
        }
        if name.eq_ignore_ascii_case("transfer-encoding") || name.eq_ignore_ascii_case("origin") {
            return Err("Browser and chunked requests are refused.");
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err("Duplicate content length.");
            }
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "Invalid content length.")?,
            );
        }
    }
    let length = length
        .filter(|l| *l <= LIMIT)
        .ok_or("Invalid content length.")?;
    Ok((end, length))
}
async fn client(
    mut stream: TcpStream,
    peer: SocketAddr,
    config: Config,
    state: Shared,
    cancel: CancellationToken,
) -> std::io::Result<()> {
    if !authorized(peer, &config.peer, cancel.clone()).await {
        return respond(&mut stream, "403 Forbidden", b"{}").await;
    }
    let mut data = Vec::new();
    let mut buffer = [0u8; 4096];
    let parsed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let n = stream.read(&mut buffer).await?;
            if n == 0 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            data.extend_from_slice(&buffer[..n]);
            if data.len() > LIMIT + 8192 {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            if data.windows(4).any(|b| b == b"\r\n\r\n") {
                let (end, size) = headers(&data).map_err(|_| std::io::ErrorKind::InvalidData)?;
                if data.len() >= end + size {
                    return serde_json::from_slice::<Value>(&data[end..end + size])
                        .map_err(std::io::Error::other);
                }
            } else if data.len() > 8192 {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
        }
    })
    .await;
    let request = match parsed {
        Ok(Ok(v)) => v,
        _ => return respond(&mut stream, "400 Bad Request", b"{}").await,
    };
    if request.get("id").is_none() {
        return respond(&mut stream, "202 Accepted", b"").await;
    }
    let response = rpc(&request, &config, &state, cancel).await;
    tokio::time::timeout(
        Duration::from_secs(5),
        respond(&mut stream, "200 OK", response.to_string().as_bytes()),
    )
    .await
    .map_err(|_| std::io::ErrorKind::TimedOut)?
}
pub async fn serve(config: Config, state: Shared, cancel: CancellationToken) -> Result<()> {
    let mut cmd = Command::new("tailscale");
    cmd.args(["ip", "-4"]);
    let bytes = common::command(cmd, vec![], Duration::from_secs(5), 1024, cancel.clone())
        .await
        .map_err(|_| "Tailscale is unavailable.")?;
    let ip = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|v| v.trim().parse::<IpAddr>().ok())
        .filter(|ip| matches!(ip,IpAddr::V4(v) if u32::from(*v)&0xffc00000==0x64400000))
        .ok_or("Tailscale IPv4 is unavailable.")?;
    let listener = TcpListener::bind((ip, config.port))
        .await
        .map_err(|_| "Could not bind the tailnet-only MCP endpoint.")?;
    let slots = Arc::new(tokio::sync::Semaphore::new(8));
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _=cancel.cancelled()=>break,
            Some(_)=tasks.join_next()=>{},
            accepted=listener.accept()=>{let (stream,peer)=accepted.map_err(|_| "Hermes MCP endpoint failed.")?;
                if let Ok(permit)=slots.clone().try_acquire_owned() {let c=config.clone();let s=state.clone();let stop=cancel.clone();tasks.spawn(async move {let _permit=permit;let _=client(stream,peer,c,s,stop).await;});}
            }
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn mcp_initialization_catalog_and_permission_boundary() {
        let config = Config {
            gateway: "http://hermes:9119".into(),
            peer: "hermes".into(),
            port: 8766,
            flake: "/tmp/flake".into(),
            services: vec![],
            rebuild: false,
        };
        let state = Arc::new(tokio::sync::Mutex::new(super::super::State::new()));
        let invoke = |method: &str, params: Value| json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
        let initialized = rpc(
            &invoke("initialize", json!({})),
            &config,
            &state,
            CancellationToken::new(),
        )
        .await;
        assert_eq!(initialized["result"]["protocolVersion"], "2025-03-26");
        let listed = rpc(
            &invoke("tools/list", json!({})),
            &config,
            &state,
            CancellationToken::new(),
        )
        .await;
        assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 9);
        for name in ["approve", "request_rebuild"] {
            let reply = rpc(
                &invoke("tools/call", json!({"name":name,"arguments":{}})),
                &config,
                &state,
                CancellationToken::new(),
            )
            .await;
            assert_eq!(reply["result"]["isError"], true);
        }
        assert!(state.lock().await.pending.is_empty());
        let reply = rpc(
            &invoke("approve", json!({})),
            &config,
            &state,
            CancellationToken::new(),
        )
        .await;
        assert_eq!(reply["error"]["code"], -32601);
    }
    #[test]
    fn tailnet_identity_not_forwarded_headers() {
        assert!(peer_matches(
            &json!({"Node":{"Name":"hermes.tail.example."}}),
            "hermes"
        ));
        assert!(!peer_matches(
            &json!({"Node":{"Name":"hermes-evil.tail.example"}}),
            "hermes"
        ));
        assert!(!peer_matches(&json!({"Name":"hermes"}), "hermes"));
        assert!(!peer_matches(
            &json!({"Node":{"Name":"hermes.other"}}),
            "hermes.tail.example"
        ));
    }
    #[test]
    fn bounded_http_rejects_smuggling_and_browser_origins() {
        assert_eq!(
            headers(b"POST /mcp HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}"),
            Ok((41, 2))
        );
        for extra in [
            "Content-Length: 2\r\nContent-Length: 2",
            "Transfer-Encoding: chunked\r\nContent-Length: 2",
            "Origin: https://evil.test\r\nContent-Length: 2",
            "Content-Length: 65537",
            " Origin: https://evil.test\r\nContent-Length: 2",
        ] {
            assert!(headers(format!("POST /mcp HTTP/1.1\r\n{extra}\r\n\r\n").as_bytes()).is_err());
        }
    }
}
