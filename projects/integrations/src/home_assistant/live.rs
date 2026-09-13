//! Single-owner state machine: UI requests and confirmed device events cannot race.
use super::connection::connection;
use super::*;
use crate::common::read_frame;
use std::{
    collections::{BTreeMap, HashMap},
    time::Instant,
};
use tokio::{io::BufReader, sync::mpsc};
pub(super) const DISCONNECTED: &str = "Home Assistant is disconnected. Reconnecting…";
pub(super) const FAILED: &str = "Home Assistant could not complete the request.";

pub(super) enum Event {
    Snapshot(BTreeMap<String, Value>, HashMap<String, String>),
    State(String, Value),
    Offline(&'static str),
    Reply(u64, bool),
}
pub(super) struct Call {
    pub(super) serial: u64,
    pub(super) data: Value,
}
struct Pending {
    desired: Value,
    request: Value,
    serial: u64,
    deadline: Instant,
    accepted: bool,
}
struct Live {
    config: Option<Config>,
    states: BTreeMap<String, Value>,
    rooms: HashMap<String, String>,
    connected: bool,
    error: &'static str,
    catalog: bool,
    pending: HashMap<String, Pending>,
    serial: u64,
    connection: Option<CancellationToken>,
    calls: Option<mpsc::Sender<Call>>,
    events: mpsc::Sender<(u64, Event)>,
    generation: u64,
    tasks: tokio::task::JoinSet<()>,
    cancel: CancellationToken,
}
fn number(value: &Value, fallback: f64) -> f64 {
    value.as_f64().filter(|v| v.is_finite()).unwrap_or(fallback)
}
impl Live {
    fn entry(&self, id: &str, selected: &Value) -> Value {
        let item = self.states.get(id).unwrap_or(&Value::Null);
        let attrs = &item["attributes"];
        let token = self.config.as_ref().map_or("", |c| c.token.as_str());
        let state = item["state"].as_str().unwrap_or("unavailable");
        let mut display_state = common::clean(&json!(state), token, 120);
        if id.starts_with("sensor.") {
            if let Ok(value) = display_state.parse::<f64>() {
                if value.is_finite() {
                    display_state = format!("{value:.1}").trim_end_matches(".0").to_owned();
                }
            }
        }
        let name = selected["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(json_string)
            .unwrap_or_else(|| attrs.get("friendly_name").cloned().unwrap_or(json!(id)));
        let room = selected["room"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.rooms.get(id).map_or("Unassigned", String::as_str));
        let modes = attrs["supported_color_modes"].as_array();
        let mode = |s: &str| modes.is_some_and(|v| v.iter().any(|m| m == s));
        let min = number(&attrs["min_color_temp_kelvin"], 2000.);
        let max = number(&attrs["max_color_temp_kelvin"], 6500.);
        let features = number(&attrs["supported_features"], 0.) as u64;
        json!({"entity_id":id,"name":common::clean(&name,token,120),"state":display_state,"unit":common::clean(&attrs["unit_of_measurement"],token,24),
            "room":common::clean(&json!(room),token,120),"favorite":selected["favorite"].as_bool().unwrap_or(false),
            "device_class":attrs["device_class"].as_str().filter(|s| matches!(*s,"temperature"|"humidity")).unwrap_or(""),
            "available":!matches!(state,"unknown"|"unavailable"),"controllable":allowed(id)&&matches!(state,"on"|"off"),
            "speed_control":id.starts_with("fan.")&&features&1!=0,"percentage":number(&attrs["percentage"],0.),"percentage_step":number(&attrs["percentage_step"],1.).clamp(1.,100.),
            "dimmable":id.starts_with("light.")&&["brightness","color_temp","hs","xy","rgb","rgbw","rgbww","white"].iter().any(|s| mode(s)),
            "temperature":id.starts_with("light.")&&mode("color_temp")&&0.<min&&min<max,"brightness":(number(&attrs["brightness"],0.)*100./255.).round_ties_even(),
            "kelvin":number(&attrs["color_temp_kelvin"],min),"min_kelvin":min,"max_kelvin":max})
    }
    fn frames(&self) -> Vec<Value> {
        let selected = self
            .config
            .as_ref()
            .map(|c| c.entities.clone())
            .unwrap_or_default();
        let entries: Vec<_> = selected
            .iter()
            .map(|s| self.entry(s["entity_id"].as_str().unwrap(), s))
            .collect();
        let summary = self.config.as_ref().map_or("", |c| c.summary.as_str());
        let summary_text = entries
            .iter()
            .find(|e| e["entity_id"] == summary && e["available"] == true)
            .map(|e| {
                format!(
                    "{} {}",
                    e["state"].as_str().unwrap_or(""),
                    e["unit"].as_str().unwrap_or("")
                )
                .trim()
                .to_owned()
            })
            .unwrap_or_default();
        let pending: serde_json::Map<_, _> = self
            .pending
            .iter()
            .map(|(id, p)| (id.clone(), p.desired.clone()))
            .collect();
        let mut frames = vec![
            json!({"ready":true,"configured":self.config.is_some(),"connected":self.connected,"entities":entries,"preferences":selected,"pending":pending,"error":self.error,"summary":summary,"summary_text":summary_text,"url":self.config.as_ref().map_or("",|c| c.url.as_str())}),
        ];
        if self.catalog {
            frames.push(json!({"catalog":self.states.keys().filter(|id| valid_entity(id)).map(|id|self.entry(id,&Value::Null)).collect::<Vec<_>>()}));
        }
        frames
    }
    async fn publish(&self) -> std::io::Result<()> {
        for frame in self.frames() {
            common::emit(&frame).await?;
        }
        Ok(())
    }
    async fn reply(&self, request: &Value, result: Result<()>) -> std::io::Result<()> {
        let frame = match result {
            Ok(()) => json!({"request":request,"ok":true}),
            Err(error) => json!({"request":request,"ok":false,"error":error}),
        };
        common::emit(&frame).await
    }
    async fn restart(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(cancel) = self.connection.take() {
            cancel.cancel();
        }
        while self.tasks.join_next().await.is_some() {}
        self.connected = false;
        self.calls = None;
        if let Some(config) = self.config.clone() {
            let cancel = self.cancel.child_token();
            self.connection = Some(cancel.clone());
            let events = self.events.clone();
            let (tx, rx) = mpsc::channel(32);
            self.calls = Some(tx);
            self.tasks
                .spawn(connection(config, events, rx, cancel, self.generation));
        }
    }
    async fn bootstrap(&mut self) {
        match load_config(self.cancel.clone()).await {
            Ok(mut config) => {
                if let Some(c) = &mut config {
                    if c.legacy {
                        if let Err(error) =
                            secret("store", &c.url, Some(&c.token), self.cancel.clone())
                                .await
                                .and_then(|_| save_config(c))
                        {
                            self.error = error;
                            return;
                        }
                        c.legacy = false;
                    }
                }
                self.config = config;
                self.error = "";
                self.restart().await;
            }
            Err(error) => {
                self.config = None;
                self.error = error;
            }
        }
    }
    async fn setup(&mut self, message: &mut Value) -> Result<()> {
        let url = message["url"]
            .as_str()
            .unwrap_or("")
            .trim()
            .trim_end_matches('/')
            .to_owned();
        let token = Zeroizing::new(message["token"].take().as_str().unwrap_or("").to_owned());
        if !valid_url(&url) || !valid_token(&token) {
            return Err("Enter a valid server URL and access token.");
        }
        let mut config = Config {
            url,
            token,
            entities: vec![],
            summary: String::new(),
            legacy: false,
        };
        if let Some(old) = &self.config {
            if old.url == config.url {
                config.entities = old.entities.clone();
                config.summary = old.summary.clone();
            }
        }
        tokio::select! { r=request(&config,"/api/",None)=>{r?;}, _=self.cancel.cancelled()=>return Err(FAILED) }
        secret(
            "store",
            &config.url,
            Some(&config.token),
            self.cancel.clone(),
        )
        .await?;
        save_config(&config)?;
        self.config = Some(config);
        self.states.clear();
        self.error = "";
        self.restart().await;
        Ok(())
    }
    fn preferences(&mut self, message: &Value) -> Result<()> {
        let config = self
            .config
            .as_ref()
            .ok_or("Connect Home Assistant first.")?;
        let mut next = config.clone();
        next.entities = selected(&message["entities"], &config.token)
            .map_err(|_| "Invalid entity selection.")?;
        next.summary = message["summary"].as_str().unwrap_or("").to_owned();
        if !next.summary.is_empty() && !next.entities.iter().any(|v| v["entity_id"] == next.summary)
        {
            return Err("Choose a selected entity for the menu bar.");
        }
        save_config(&next)?;
        self.config = Some(next);
        Ok(())
    }
    fn control(&mut self, message: &Value) -> Result<()> {
        let id = message["entity_id"]
            .as_str()
            .ok_or("This device is not selected or is disconnected.")?;
        if !self.connected
            || !self
                .config
                .as_ref()
                .is_some_and(|c| c.entities.iter().any(|v| v["entity_id"] == id))
        {
            return Err("This device is not selected or is disconnected.");
        }
        let entry = self.entry(id, &Value::Null);
        if entry["available"] != true
            || entry["controllable"] != true
            || self.pending.contains_key(id)
        {
            return Err("This device is unavailable, read-only or still updating.");
        }
        let desired = message["desired"]
            .as_object()
            .filter(|d| {
                !d.is_empty()
                    && d.keys().all(|k| {
                        matches!(k.as_str(), "state" | "brightness" | "kelvin" | "percentage")
                    })
            })
            .ok_or("Unsupported device control.")?;
        let mut service = "turn_on".to_owned();
        let mut body = serde_json::Map::new();
        if let Some(state) = desired.get("state") {
            if desired.len() != 1 || !matches!(state.as_str(), Some("on" | "off")) {
                return Err("Choose an explicit on or off state.");
            }
            service = format!("turn_{}", state.as_str().unwrap());
        }
        if desired.contains_key("percentage") {
            if desired.len() != 1 {
                return Err("Choose one fan control at a time.");
            }
            service = "set_percentage".to_owned();
        }
        for (key, capability, min, max, field) in [
            ("percentage", "speed_control", 0., 100., "percentage"),
            ("brightness", "dimmable", 1., 100., "brightness_pct"),
            (
                "kelvin",
                "temperature",
                number(&entry["min_kelvin"], 2000.),
                number(&entry["max_kelvin"], 6500.),
                "color_temp_kelvin",
            ),
        ] {
            if let Some(value) = desired.get(key) {
                let value = value
                    .as_f64()
                    .filter(|v| v.is_finite() && *v >= min && *v <= max)
                    .ok_or("This device does not support that value.")?;
                if entry[capability] != true {
                    return Err("This device does not support that value.");
                }
                body.insert(field.into(), json!(value.round_ties_even()));
            }
        }
        self.serial += 1;
        let serial = self.serial;
        let data = json!({"type":"call_service","domain":id.split('.').next().unwrap(),"service":service,"service_data":body,"target":{"entity_id":id}});
        self.calls
            .as_ref()
            .ok_or(DISCONNECTED)?
            .try_send(Call { serial, data })
            .map_err(|_| "Home Assistant is busy. Retry shortly.")?;
        self.pending.insert(
            id.to_owned(),
            Pending {
                desired: message["desired"].clone(),
                request: message["request"].clone(),
                serial,
                deadline: Instant::now() + Duration::from_secs(12),
                accepted: false,
            },
        );
        self.error = "";
        Ok(())
    }
    async fn settle(&mut self) -> std::io::Result<()> {
        let mut finished = vec![];
        for (id, pending) in &self.pending {
            let entry = self.entry(id, &Value::Null);
            let matches = pending
                .desired
                .as_object()
                .unwrap()
                .iter()
                .all(|(key, value)| {
                    if matches!(key.as_str(), "brightness" | "kelvin" | "percentage") {
                        (number(&entry[key], -99999.) - number(value, 99999.)).abs()
                            <= if key == "kelvin" { 50. } else { 1. }
                    } else {
                        entry[key] == *value
                    }
                });
            if pending.accepted && matches {
                finished.push((id.clone(), Ok(())));
            } else if Instant::now() >= pending.deadline {
                finished.push((
                    id.clone(),
                    Err(
                        "The device has not confirmed the change. Check its state before retrying.",
                    ),
                ));
            }
        }
        for (id, result) in finished {
            let p = self.pending.remove(&id).unwrap();
            if let Err(error) = result {
                self.error = error;
            }
            self.reply(&p.request, result).await?;
            self.publish().await?;
        }
        Ok(())
    }
    async fn fail_pending(&mut self, error: &'static str) -> std::io::Result<()> {
        for (_, pending) in std::mem::take(&mut self.pending) {
            self.reply(&pending.request, Err(error)).await?;
        }
        Ok(())
    }
}
fn json_string(s: &str) -> Value {
    json!(s)
}

pub async fn watch(cancel: CancellationToken) -> std::io::Result<()> {
    let (events_tx, mut events) = mpsc::channel(128);
    let (input_tx, mut input) = mpsc::channel(40);
    let reader_cancel = cancel.clone();
    let reader = tokio::spawn(async move {
        let mut stdin = BufReader::new(common::FdIo::stdin().expect("stdin descriptor"));
        loop {
            let frame = tokio::select! {frame=read_frame(&mut stdin,MAX_CONFIG)=>frame,_=reader_cancel.cancelled()=>return};
            match frame {
                Ok(Some(bytes)) => {
                    let message = serde_json::from_slice::<Value>(&bytes);
                    if input_tx.send(message).await.is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
        reader_cancel.cancel();
    });
    let mut live = Live {
        config: None,
        states: BTreeMap::new(),
        rooms: HashMap::new(),
        connected: false,
        error: "",
        catalog: false,
        pending: HashMap::new(),
        serial: 0,
        connection: None,
        calls: None,
        events: events_tx,
        generation: 0,
        tasks: tokio::task::JoinSet::new(),
        cancel: cancel.clone(),
    };
    live.publish().await?;
    live.bootstrap().await;
    live.publish().await?;
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let mut heartbeat = Instant::now();
    let result=async {loop {tokio::select!{
        _=cancel.cancelled()=>break,
        _=tick.tick()=>{live.settle().await?;if heartbeat.elapsed()>=Duration::from_secs(20){live.publish().await?;heartbeat=Instant::now();}},
        event=events.recv()=>{let mut publish = true;let event = match event {Some((generation,event)) if generation == live.generation => Some(event),Some(_) => continue,None=>None};match event {
            Some(Event::Snapshot(states,rooms))=>{live.states=states;live.rooms=rooms;live.connected=true;live.error="";},
            Some(Event::State(id,value))=>{publish = live.catalog || live.config.as_ref().is_some_and(|c| c.entities.iter().any(|e|e["entity_id"]==id));if value.is_null(){live.states.remove(&id);}else{live.states.insert(id,value);}},
            Some(Event::Offline(error))=>{live.connected=false;live.error=error;live.fail_pending("Connection lost. Check the device before retrying.").await?;},
            Some(Event::Reply(serial,success))=>{let id=live.pending.iter().find(|(_,p)|p.serial==serial).map(|(id,_)|id.clone());if let Some(id)=id{if success{live.pending.get_mut(&id).unwrap().accepted=true;}else{let p=live.pending.remove(&id).unwrap();live.error=FAILED;live.reply(&p.request,Err(FAILED)).await?;}}},None=>break
        }live.settle().await?;if publish { live.publish().await?; }},
        message=input.recv()=>{let Some(message)=message else{break;};let mut message=match message{Ok(v) if v.is_object()=>v,_=>{common::emit(&json!({"ok":false,"error":"Invalid Home Assistant request."})).await?;continue;}};
            if !message["request"].is_null() && message["request"].as_i64().is_none() {common::emit(&json!({"ok":false,"request":null,"error":"Invalid Home Assistant request."})).await?;continue;}
            let request=message["request"].clone();let action=message["action"].as_str().unwrap_or("").to_owned();
            let result=match action.as_str(){
                "setup"|"preferences" if !live.pending.is_empty()=>Err("Wait for device changes to finish before editing settings."),
                "setup"=>live.setup(&mut message).await,
                "preferences"=>live.preferences(&message),
                "set"=>live.control(&message),
                "catalog"=>{live.catalog=message["open"].as_bool().unwrap_or(true);Ok(())},
                "refresh"=>{if live.config.is_none(){live.bootstrap().await;}else if !live.connected{live.restart().await;}Ok(())},
                _=>Err("Unknown Home Assistant action.")
            };
            if let Err(error)=result{live.error=error;}
            if action!="set"||result.is_err(){live.reply(&request,result).await?;}live.publish().await?;
        }
    }}Ok(())}.await;
    cancel.cancel();
    if let Some(connection) = live.connection.take() {
        connection.cancel();
    }
    while live.tasks.join_next().await.is_some() {}
    reader.abort();
    let _ = reader.await;
    result
}
