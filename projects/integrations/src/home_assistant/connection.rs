//! Native TLS/WebSocket transport with explicit reconnect generations.
use super::live::{Call, Event, DISCONNECTED, FAILED};
use super::{valid_entity, Config, MAX_RESPONSE};
use crate::common::{self, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio_tungstenite::{
    tungstenite::{protocol::WebSocketConfig, Message},
    MaybeTlsStream, WebSocketStream,
};
use tokio_util::sync::CancellationToken;
type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
async fn receive(ws: &mut Socket) -> Result<Value> {
    loop {
        match tokio::time::timeout(Duration::from_secs(45), ws.next())
            .await
            .map_err(|_| DISCONNECTED)?
            .ok_or(DISCONNECTED)?
            .map_err(|_| DISCONNECTED)?
        {
            Message::Text(text) => return serde_json::from_str(&text).map_err(|_| FAILED),
            Message::Ping(bytes) => {
                ws.send(Message::Pong(bytes))
                    .await
                    .map_err(|_| DISCONNECTED)?;
            }
            Message::Pong(_) => {}
            _ => return Err(DISCONNECTED),
        }
    }
}
async fn send(ws: &mut Socket, value: &Value) -> Result<()> {
    ws.send(Message::text(value.to_string()))
        .await
        .map_err(|_| DISCONNECTED)
}
/// Retain only fields rendered or used by the control allowlist. An event stream
/// cannot accumulate arbitrary multi-megabyte entity attributes over time.
fn state_projection(value: &Value, token: &str) -> Value {
    let mut attrs = serde_json::Map::new();
    let source = &value["attributes"];
    for (key, limit) in [
        ("friendly_name", 120),
        ("unit_of_measurement", 24),
        ("device_class", 32),
    ] {
        if source.get(key).is_some() {
            attrs.insert(key.into(), json!(common::clean(&source[key], token, limit)));
        }
    }
    for key in [
        "supported_features",
        "min_color_temp_kelvin",
        "max_color_temp_kelvin",
        "percentage",
        "percentage_step",
        "brightness",
        "color_temp_kelvin",
    ] {
        if let Some(number) = source[key].as_f64().filter(|v| v.is_finite()) {
            attrs.insert(key.into(), json!(number));
        }
    }
    let modes: Vec<_> = [
        "brightness",
        "color_temp",
        "hs",
        "xy",
        "rgb",
        "rgbw",
        "rgbww",
        "white",
    ]
    .into_iter()
    .filter(|mode| {
        source["supported_color_modes"]
            .as_array()
            .is_some_and(|values| values.iter().any(|v| v == mode))
    })
    .collect();
    attrs.insert("supported_color_modes".into(), json!(modes));
    json!({"state":common::clean(&json!(value["state"].as_str().unwrap_or("unavailable")),token,120), "attributes":attrs})
}
fn apply_event(
    value: &Value,
    states: &mut BTreeMap<String, Value>,
    token: &str,
) -> Option<(String, Value)> {
    if value["type"] != "event" {
        return None;
    }
    let event = &value["event"]["data"];
    let id = event["entity_id"].as_str().filter(|id| valid_entity(id))?;
    let new = if event["new_state"].is_object() {
        state_projection(&event["new_state"], token)
    } else if event["new_state"].is_null() {
        Value::Null
    } else {
        return None;
    };
    if new.is_null() {
        states.remove(id);
    } else if new.is_object() && (states.contains_key(id) || states.len() < 16384) {
        states.insert(id.to_owned(), new.clone());
    } else {
        return None;
    }
    Some((id.to_owned(), new))
}
async fn rpc(
    ws: &mut Socket,
    serial: &mut u64,
    kind: &str,
    states: &mut BTreeMap<String, Value>,
    token: &str,
) -> Result<Value> {
    *serial += 1;
    let id = *serial;
    let mut call = json!({"id":id,"type":kind});
    if kind == "subscribe_events" {
        call["event_type"] = json!("state_changed");
    }
    send(ws, &call).await?;
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let value = receive(ws).await?;
            if value["type"] == "result" && value["id"] == id {
                if value["success"] != true {
                    return Err(FAILED);
                }
                if kind == "get_states" {
                    let rows = value["result"]
                        .as_array()
                        .ok_or("Home Assistant returned an invalid state list.")?;
                    states.clear();
                    for item in rows.iter().take(16384) {
                        if let Some(id) = item["entity_id"].as_str().filter(|id| valid_entity(id)) {
                            states.insert(id.to_owned(), state_projection(item, token));
                        }
                    }
                }
                return Ok(value["result"].clone());
            }
            apply_event(&value, states, token);
        }
    })
    .await
    .map_err(|_| FAILED)?
}
async fn connected(
    config: &Config,
    events: &mpsc::Sender<(u64, Event)>,
    generation: u64,
    calls: &mut mpsc::Receiver<Call>,
) -> Result<()> {
    let mut url = url::Url::parse(&format!("{}/api/websocket", config.url)).map_err(|_| FAILED)?;
    let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
    url.set_scheme(scheme).map_err(|_| FAILED)?;
    let settings = WebSocketConfig::default()
        .max_message_size(Some(MAX_RESPONSE))
        .max_frame_size(Some(MAX_RESPONSE))
        .max_write_buffer_size(MAX_RESPONSE);
    // tungstenite performs one upgrade and never follows redirects; no proxy environment is read.
    let (mut ws, _) = tokio::time::timeout(
        Duration::from_secs(8),
        tokio_tungstenite::connect_async_with_config(url.as_str(), Some(settings), true),
    )
    .await
    .map_err(|_| DISCONNECTED)?
    .map_err(|_| DISCONNECTED)?;
    tokio::time::timeout(Duration::from_secs(12), async {
        if receive(&mut ws).await?["type"] != "auth_required" {
            return Err("Home Assistant did not accept the connection.");
        }
        send(
            &mut ws,
            &json!({"type":"auth","access_token":config.token.as_str()}),
        )
        .await?;
        if receive(&mut ws).await?["type"] != "auth_ok" {
            return Err("Access was denied. Update the token in setup.");
        }
        Ok(())
    })
    .await
    .map_err(|_| DISCONNECTED)??;
    let mut serial = 0;
    let mut states = BTreeMap::new();
    rpc(
        &mut ws,
        &mut serial,
        "subscribe_events",
        &mut states,
        &config.token,
    )
    .await?;
    rpc(
        &mut ws,
        &mut serial,
        "get_states",
        &mut states,
        &config.token,
    )
    .await?;
    let mut rooms = HashMap::new();
    if let Ok(areas) = rpc(
        &mut ws,
        &mut serial,
        "config/area_registry/list",
        &mut states,
        &config.token,
    )
    .await
    {
        if let Ok(devices) = rpc(
            &mut ws,
            &mut serial,
            "config/device_registry/list",
            &mut states,
            &config.token,
        )
        .await
        {
            if let Ok(registry) = rpc(
                &mut ws,
                &mut serial,
                "config/entity_registry/list",
                &mut states,
                &config.token,
            )
            .await
            {
                let areas: HashMap<_, _> = areas
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|a| Some((a["area_id"].as_str()?, a["name"].as_str()?)))
                    .collect();
                let devices: HashMap<_, _> = devices
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|d| Some((d["id"].as_str()?, d["area_id"].as_str()?)))
                    .collect();
                for item in registry.as_array().into_iter().flatten().take(16384) {
                    if let Some(id) = item["entity_id"].as_str().filter(|id| valid_entity(id)) {
                        let area = item["area_id"].as_str().or_else(|| {
                            item["device_id"]
                                .as_str()
                                .and_then(|id| devices.get(id).copied())
                        });
                        rooms.insert(
                            id.to_owned(),
                            area.and_then(|id| areas.get(id))
                                .copied()
                                .unwrap_or("Unassigned")
                                .to_owned(),
                        );
                    }
                }
            }
        }
    }
    events
        .send((generation, Event::Snapshot(states.clone(), rooms)))
        .await
        .map_err(|_| DISCONNECTED)?;
    let mut replies = HashMap::new();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    heartbeat.tick().await;
    let mut last_seen = Instant::now();
    loop {
        tokio::select! {
            message=ws.next()=>{
                last_seen=Instant::now();
                match message.ok_or(DISCONNECTED)?.map_err(|_|DISCONNECTED)? {
                    Message::Text(text)=>{
                        let value:Value=serde_json::from_str(&text).map_err(|_|FAILED)?;
                        if value["type"]=="result"{if let Some(wire_id)=value["id"].as_u64(){if let Some((id,_))=replies.remove(&wire_id){events.send((generation,Event::Reply(id,value["success"]==true))).await.map_err(|_|DISCONNECTED)?;}}}
                        if let Some((id,value))=apply_event(&value,&mut states,&config.token){events.send((generation,Event::State(id,value))).await.map_err(|_|DISCONNECTED)?;}
                    },Message::Ping(data)=>{ws.send(Message::Pong(data)).await.map_err(|_|DISCONNECTED)?;},Message::Pong(_)=>{},_=>return Err(DISCONNECTED)
                }
            },
            call=calls.recv()=>{let Some(mut call)=call else{return Err(DISCONNECTED);};serial+=1;call.data["id"]=json!(serial);replies.insert(serial,(call.serial,Instant::now()));send(&mut ws,&call.data).await?;},
            _=heartbeat.tick()=>{
                if last_seen.elapsed()>Duration::from_secs(45){return Err(DISCONNECTED);}
                replies.retain(|_,(_,time)|time.elapsed()<Duration::from_secs(15));
                ws.send(Message::Ping(Vec::new().into())).await.map_err(|_|DISCONNECTED)?;
            }
        }
    }
}
pub(super) async fn connection(
    config: Config,
    events: mpsc::Sender<(u64, Event)>,
    mut calls: mpsc::Receiver<Call>,
    cancel: CancellationToken,
    generation: u64,
) {
    let mut delay = 1;
    loop {
        let began = Instant::now();
        let result = tokio::select! {result=connected(&config,&events,generation,&mut calls)=>result,_=cancel.cancelled()=>return};
        if began.elapsed() > Duration::from_secs(30) {
            delay = 1;
        }
        if events
            .send((
                generation,
                Event::Offline(result.err().unwrap_or(DISCONNECTED)),
            ))
            .await
            .is_err()
        {
            return;
        }
        // A command whose transport died is never silently replayed on a new connection.
        while let Ok(call) = calls.try_recv() {
            let _ = events
                .send((generation, Event::Reply(call.serial, false)))
                .await;
        }
        tokio::select! {_=tokio::time::sleep(Duration::from_secs(delay))=>{},_=cancel.cancelled()=>return};
        delay = (delay * 2).min(30);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_state_is_bounded_and_has_no_unrendered_attributes() {
        let token = "secret-fixture-token";
        let projected = state_projection(
            &json!({"state":token,"attributes":{"friendly_name":format!("{token}\nDesk"),"private_attribute":"x".repeat(1024*1024),"brightness":"not-a-number","supported_color_modes":["rgb","invalid","rgb"],"device_class":"humidity"}}),
            token,
        );
        assert!(!projected.to_string().contains(token));
        assert!(projected.to_string().len() < 512);
        assert!(projected["attributes"].get("private_attribute").is_none());
        assert!(projected["attributes"].get("brightness").is_none());
        assert_eq!(projected["attributes"]["friendly_name"], "[redacted]Desk");
        assert_eq!(
            projected["attributes"]["supported_color_modes"],
            json!(["rgb"])
        );
    }
}
