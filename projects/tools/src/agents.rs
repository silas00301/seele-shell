use crate::command::{
    atomic_write, epoch, exec, home, output, process_alive, state_home, timestamp,
};
use crate::Result;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

fn agent_dir() -> PathBuf {
    state_home().join("seele-shell/agents")
}

fn clean_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .collect()
}

fn parse_proc_stat(text: &str) -> Option<(u32, u64, &str)> {
    let start = text.find('(')?;
    let end = text.rfind(") ")?;
    let name = text.get(start + 1..end)?;
    let mut fields = text[end + 2..].split_whitespace();
    let parent = fields.nth(1)?.parse().ok()?;
    // After state and ppid, utime is nine fields ahead; stime follows it.
    let user: u64 = fields.nth(9)?.parse().ok()?;
    let system: u64 = fields.next()?.parse().ok()?;
    Some((parent, user.checked_add(system)?, name))
}

fn proc_stat(pid: u32) -> Option<(u32, u64)> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (parent, ticks, _) = parse_proc_stat(&text)?;
    Some((parent, ticks))
}

fn normalized_process_name(name: &str) -> &str {
    name.trim()
        .trim_start_matches('.')
        .trim_end_matches("-wrapped")
        .trim_end_matches("-wrapp")
}

fn process_name(pid: u32) -> String {
    let name = fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
    normalized_process_name(&name).to_owned()
}

fn owning_pid(agent: &str) -> u32 {
    let fallback = unsafe { libc::getppid() as u32 };
    let mut pid = fallback;
    for _ in 0..12 {
        if process_name(pid) == agent {
            return pid;
        }
        let Some((parent, _)) = proc_stat(pid) else {
            break;
        };
        if parent <= 1 {
            break;
        }
        pid = parent;
    }
    fallback
}

pub fn hook(arguments: &[String]) -> Result {
    let agent = arguments.first().ok_or("agent required")?;
    let event = arguments.get(1).ok_or("event required")?;
    let mut payload = String::new();
    if !io::stdin().is_terminal() {
        io::stdin().read_to_string(&mut payload)?;
    }
    let key = serde_json::from_str::<Value>(&payload)
        .ok()
        .and_then(|value| value.get("session_id")?.as_str().map(clean_key))
        .filter(|value| !value.is_empty());
    let pid = owning_pid(agent);
    let key = key.unwrap_or_else(|| pid.to_string());
    let path = agent_dir().join(format!("{agent}-native-{key}.json"));
    if event == "end" {
        let _ = fs::remove_file(path);
        return Ok(());
    }
    let now = timestamp();
    let started = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("startedAt")?.as_str().map(str::to_owned))
        .unwrap_or_else(|| now.clone());
    let record = json!({
        "agent": agent,
        "status": event,
        "source": "native",
        "pid": pid,
        "startedAt": started,
        "updatedAt": now
    });
    atomic_write(&path, serde_json::to_string(&record)?.as_bytes())
}

fn subtree_ticks(root: u32) -> Option<u64> {
    let mut processes = Vec::new();
    for entry in fs::read_dir("/proc").ok()? {
        let pid = entry
            .ok()?
            .file_name()
            .to_string_lossy()
            .parse::<u32>()
            .ok();
        if let Some(pid) = pid {
            if let Some((parent, ticks)) = proc_stat(pid) {
                processes.push((pid, parent, ticks));
            }
        }
    }
    Some(subtree_ticks_in(root, &processes))
}

fn subtree_ticks_in(root: u32, processes: &[(u32, u32, u64)]) -> u64 {
    ProcessTree::new(processes).subtree_ticks(root)
}

struct ProcessTree {
    children: HashMap<u32, Vec<u32>>,
    ticks: HashMap<u32, u64>,
}

impl ProcessTree {
    fn new(processes: &[(u32, u32, u64)]) -> Self {
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut ticks = HashMap::new();
        for &(pid, parent, count) in processes {
            children.entry(parent).or_default().push(pid);
            *ticks.entry(pid).or_default() += count;
        }
        Self { children, ticks }
    }

    fn subtree_ticks(&self, root: u32) -> u64 {
        let mut pending = vec![root];
        let mut visited = HashSet::new();
        let mut total = 0;
        while let Some(pid) = pending.pop() {
            if !visited.insert(pid) {
                continue;
            }
            total += self.ticks.get(&pid).copied().unwrap_or(0);
            if let Some(children) = self.children.get(&pid) {
                pending.extend(children);
            }
        }
        total
    }
}

fn write_run_state(
    path: &Path,
    agent: &str,
    status: &str,
    started: &str,
    ended: Option<&str>,
    code: Option<i32>,
) -> Result {
    let mut value = json!({
        "agent": agent,
        "status": status,
        "source": "heuristic",
        "startedAt": started,
        "updatedAt": timestamp(),
        "pid": std::process::id()
    });
    if let Some(ended) = ended {
        value["endedAt"] = json!(ended);
        value["exitCode"] = json!(code);
    }
    atomic_write(path, serde_json::to_string(&value)?.as_bytes())
}

pub fn run_agent(arguments: &[String]) -> Result {
    let agent = arguments.first().ok_or("agent required")?;
    let command = arguments.get(1).ok_or("command required")?;
    let command_arguments = &arguments[2..];
    let directory = agent_dir();
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{agent}-heuristic-{}.json", std::process::id()));
    let started = timestamp();
    write_run_state(&path, agent, "working", &started, None, None)?;
    let mut child = Command::new(command).args(command_arguments).spawn()?;
    let mut last_ticks = None;
    let mut idle = 0;
    let mut current = "working";
    loop {
        if let Some(status) = child.try_wait()? {
            let code = status.code().unwrap_or(128);
            let ended = timestamp();
            write_run_state(&path, agent, "finished", &started, Some(&ended), Some(code))?;
            std::process::exit(code);
        }
        let ticks = subtree_ticks(child.id());
        if ticks.is_some() && ticks == last_ticks {
            idle += 2;
        } else {
            idle = 0;
            last_ticks = ticks;
        }
        let next = if idle >= 20 { "input" } else { "working" };
        if next != current {
            current = next;
            write_run_state(&path, agent, current, &started, None, None)?;
        }
        thread::sleep(Duration::from_secs(2));
    }
}

pub fn launch(arguments: &[String]) -> Result {
    let agent = arguments.first().map(String::as_str).unwrap_or("pi");
    let prompt = arguments.get(1..).unwrap_or_default().join(" ");
    let active_pid = output("hyprctl", ["activewindow", "-j"])
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("pid")?.as_u64())
        .map(|pid| pid as u32);
    let cwd = active_pid
        .and_then(|pid| fs::canonicalize(format!("/proc/{pid}/cwd")).ok())
        .filter(|path| path.is_dir())
        .unwrap_or_else(home);
    let variable = match agent {
        "pi" => "SEELE_SHELL_PI",
        "opencode" => "SEELE_SHELL_OPENCODE",
        "codex" => "SEELE_SHELL_CODEX",
        "claude" => "SEELE_SHELL_CLAUDE",
        _ => return Err(format!("Unknown agent: {agent}").into()),
    };
    let default = agent;
    let harness = env::var(variable).unwrap_or_else(|_| default.to_owned());
    let mut inner = vec![harness];
    if agent == "opencode" {
        inner.push(cwd.to_string_lossy().into_owned());
        if !prompt.is_empty() {
            inner.extend(["--prompt".to_owned(), prompt]);
        }
    } else if !prompt.is_empty() {
        inner.push(prompt);
    }
    let mut command = vec![
        format!("--working-directory={}", cwd.display()),
        "--class=org.seele.agent".to_owned(),
        "-e".to_owned(),
        "seele-agent-run".to_owned(),
        agent.to_owned(),
    ];
    command.extend(inner);
    exec(
        &env::var("SEELE_SHELL_GHOSTTY").unwrap_or_else(|_| "ghostty".into()),
        &command,
    )
}

fn collect(kind: &str) -> Vec<Value> {
    let binary = env::var("SEELE_SHELL_CODEXBAR").unwrap_or_else(|_| "codexbar".into());
    let providers = env::var("SEELE_SHELL_CODEXBAR_PROVIDERS").unwrap_or_else(|_| "both".into());
    let mut records = Vec::new();
    for provider in providers.split_whitespace() {
        let mut args = vec![kind, "--provider", provider, "--json"];
        if kind == "cost" {
            args.extend(["--days", "365"]);
        }
        let value = output(&binary, args)
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .unwrap_or_else(|| json!([]));
        match value {
            Value::Array(values) => records.extend(values.into_iter().filter(Value::is_object)),
            value if value.is_object() => records.push(value),
            _ => {}
        }
    }
    records
}

fn number(value: Option<&Value>) -> f64 {
    value.and_then(Value::as_f64).unwrap_or(0.0)
}

fn daily(record: &Value) -> Vec<&Value> {
    record
        .get("daily")
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn incomplete_cost(records: &[Value]) -> bool {
    records.iter().any(|record| {
        let provider = record
            .get("provider")
            .or_else(|| record.get("source"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if !provider.eq_ignore_ascii_case("claude") {
            return false;
        }
        let tokens = number(
            record
                .get("last30DaysTokens")
                .or_else(|| record.pointer("/totals/totalTokens")),
        )
        .max(
            daily(record)
                .iter()
                .map(|day| number(day.get("totalTokens")))
                .sum(),
        );
        let cost = number(
            record
                .get("last30DaysCostUSD")
                .or_else(|| record.pointer("/totals/totalCost")),
        )
        .max(
            daily(record)
                .iter()
                .map(|day| number(day.get("totalCost")))
                .sum(),
        );
        tokens > 0.0
            && (cost <= 0.0
                || daily(record).iter().any(|day| {
                    day.get("modelBreakdowns")
                        .and_then(Value::as_array)
                        .map(|models| {
                            models.iter().any(|model| {
                                number(model.get("totalTokens")) > 0.0
                                    && number(model.get("cost")) <= 0.0
                            })
                        })
                        .unwrap_or(false)
                }))
    })
}

fn model_rows(days: &[&Value]) -> Vec<Value> {
    let mut models: HashMap<String, (f64, f64)> = HashMap::new();
    for model in days.iter().flat_map(|day| {
        day.get("modelBreakdowns")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
    }) {
        let name = model
            .get("modelName")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let entry = models.entry(name).or_default();
        entry.0 += number(model.get("totalTokens"));
        entry.1 += number(model.get("cost"));
    }
    let mut rows: Vec<Value> = models
        .into_iter()
        .map(|(name, (tokens, cost))| json!({"name":name,"tokens":tokens,"cost":cost}))
        .collect();
    rows.sort_by(|left, right| number(right.get("tokens")).total_cmp(&number(left.get("tokens"))));
    rows
}

fn period(days: &[&Value]) -> Value {
    json!({
        "totalTokens": days.iter().map(|day| number(day.get("totalTokens"))).sum::<f64>(),
        "totalCost": days.iter().map(|day| number(day.get("totalCost"))).sum::<f64>(),
        "models": model_rows(days)
    })
}

fn date_days_ago(today: &str, days: i32) -> String {
    output(
        "date",
        ["-u", "-d", &format!("{today} -{days} days"), "+%F"],
    )
    .unwrap_or_else(|| today.to_owned())
    .trim()
    .to_owned()
}

fn display_name(id: &str) -> String {
    match id {
        "codex" => "Codex".into(),
        "claude" => "Claude".into(),
        "openai" => "OpenAI".into(),
        "copilot" => "Copilot".into(),
        "cursor" => "Cursor".into(),
        "gemini" => "Gemini".into(),
        "opencode" => "OpenCode".into(),
        _ => id.to_ascii_uppercase(),
    }
}

pub fn state(_arguments: &[String]) -> Result {
    let usage_thread = thread::spawn(|| collect("usage"));
    let cost_thread = thread::spawn(|| collect("cost"));
    let usage = usage_thread.join().unwrap_or_default();
    let mut cost = cost_thread.join().unwrap_or_default();
    for _ in 0..2 {
        if !incomplete_cost(&cost) {
            break;
        }
        cost = collect("cost");
    }
    let today = env::var("SEELE_SHELL_TODAY").unwrap_or_else(|_| {
        output("date", ["+%F"])
            .unwrap_or_default()
            .trim()
            .to_owned()
    });
    let week = date_days_ago(&today, 6);
    let month = date_days_ago(&today, 29);
    let days: Vec<&Value> = cost
        .iter()
        .flat_map(daily)
        .filter(|day| day.get("date").and_then(Value::as_str).is_some())
        .collect();
    let select = |start: &str| {
        days.iter()
            .copied()
            .filter(|day| {
                let date = day.get("date").and_then(Value::as_str).unwrap_or("");
                date >= start && date <= today.as_str()
            })
            .collect::<Vec<_>>()
    };
    let day_days = days
        .iter()
        .copied()
        .filter(|day| day.get("date").and_then(Value::as_str) == Some(today.as_str()))
        .collect::<Vec<_>>();
    let week_days = select(&week);
    let month_days = select(&month);
    let mut totals: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for day in &days {
        let date = day
            .get("date")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let entry = totals.entry(date).or_default();
        entry.0 += number(day.get("totalTokens"));
        entry.1 += number(day.get("totalCost"));
    }
    let daily_totals: Vec<Value> = totals
        .into_iter()
        .map(|(date, (tokens, cost))| json!({"date":date,"totalTokens":tokens,"cost":cost}))
        .collect();
    let subscriptions: Vec<Value> = usage.iter().map(|record| {
        let id = record.get("provider").or_else(|| record.get("source")).and_then(Value::as_str).unwrap_or("unknown");
        let mut limits = Vec::new();
        for (name, pointer) in [("Session", "/usage/primary"), ("Weekly", "/usage/secondary"), ("Additional", "/usage/tertiary")] {
            if let Some(value) = record.pointer(pointer).filter(|value| value.get("usedPercent").is_some()) {
                limits.push(json!({"name":name,"usedPercent":number(value.get("usedPercent")),"resetsAt":value.get("resetsAt").and_then(Value::as_str).unwrap_or(""),"resetDescription":value.get("resetDescription").and_then(Value::as_str).unwrap_or("")}));
            }
        }
        json!({"id":id,"name":display_name(id),"plan":record.pointer("/usage/loginMethod").or_else(|| record.get("plan")).and_then(Value::as_str).unwrap_or(""),"source":record.get("source").and_then(Value::as_str).unwrap_or("unavailable"),"limits":limits,"credits":record.pointer("/credits/remaining").cloned().unwrap_or(Value::Null)})
    }).collect();
    let result = json!({
        "generatedAt": timestamp(),
        "subscriptions": subscriptions,
        "local": {
            "today": daily_totals.iter().find(|item| item.get("date").and_then(Value::as_str) == Some(today.as_str())).cloned().unwrap_or_else(|| json!({})),
            "daily": daily_totals.iter().rev().take(7).cloned().collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>(),
            "periods": {"day":period(&day_days),"week":period(&week_days),"month":period(&month_days),"all":period(&days)},
            "models": model_rows(&days),
            "totalTokens": cost.iter().map(|record| number(record.get("last30DaysTokens").or_else(|| record.pointer("/totals/totalTokens")))).sum::<f64>(),
            "totalCost": cost.iter().map(|record| number(record.get("last30DaysCostUSD").or_else(|| record.pointer("/totals/totalCost")))).sum::<f64>(),
            "currency": cost.iter().find_map(|record| record.get("currencyCode").and_then(Value::as_str)).unwrap_or("USD")
        },
        "launchers": [
            {"id":"pi","name":"Pi","command":env::var("SEELE_SHELL_PI").unwrap_or_else(|_|"pi".into()),"description":"Primary Seele coding agent"},
            {"id":"opencode","name":"OpenCode","command":env::var("SEELE_SHELL_OPENCODE").unwrap_or_else(|_|"opencode".into()),"description":"Provider-flexible coding agent"},
            {"id":"codex","name":"Codex","command":env::var("SEELE_SHELL_CODEX").unwrap_or_else(|_|"codex".into()),"description":"OpenAI Codex CLI"},
            {"id":"claude","name":"Claude Code","command":env::var("SEELE_SHELL_CLAUDE").unwrap_or_else(|_|"claude".into()),"description":"Anthropic Claude Code CLI"}
        ]
    });
    let cache = state_home().join("seele-shell/agents.json");
    let encoded = serde_json::to_vec(&result)?;
    atomic_write(&cache, &encoded)?;
    println!("{}", String::from_utf8(encoded)?);
    Ok(())
}

pub(crate) fn aggregate_states() -> Value {
    let directory = agent_dir();
    let now = epoch();
    let mut groups: HashMap<String, Vec<Value>> = HashMap::new();
    if let Ok(entries) = fs::read_dir(&directory) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json")
                || entry.file_name().to_string_lossy().starts_with('.')
            {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let Some(agent) = value
                .get("agent")
                .and_then(Value::as_str)
                .map(str::to_owned)
            else {
                continue;
            };
            let live = value
                .get("pid")
                .and_then(Value::as_u64)
                .map(|pid| process_alive(pid as u32))
                .unwrap_or(false);
            let updated = value
                .get("updatedAt")
                .or_else(|| value.get("endedAt"))
                .or_else(|| value.get("startedAt"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let at = output("date", ["-d", updated, "+%s"])
                .and_then(|text| text.trim().parse::<i64>().ok())
                .unwrap_or(0);
            if !live && now - at >= 300 {
                let _ = fs::remove_file(entry.path());
                continue;
            }
            value["live"] = json!(live);
            groups.entry(agent).or_default().push(value);
        }
    }
    for record in sampled_harnesses() {
        if let Some(agent) = record
            .get("agent")
            .and_then(Value::as_str)
            .map(str::to_owned)
        {
            groups.entry(agent).or_default().push(record);
        }
    }
    let mut result = Map::new();
    for (agent, mut records) in groups {
        records.sort_by_key(|value| {
            value
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned()
        });
        let active: Vec<&Value> = records
            .iter()
            .filter(|value| value.get("live").and_then(Value::as_bool).unwrap_or(false))
            .collect();
        let native: Vec<&Value> = active
            .iter()
            .copied()
            .filter(|value| value.get("source").and_then(Value::as_str) == Some("native"))
            .collect();
        let heuristic: Vec<&Value> = active
            .iter()
            .copied()
            .filter(|value| value.get("source").and_then(Value::as_str) == Some("heuristic"))
            .collect();
        let cpu: Vec<&Value> = active
            .iter()
            .copied()
            .filter(|value| value.get("source").and_then(Value::as_str) == Some("cpu"))
            .collect();
        let chosen = native
            .last()
            .or_else(|| heuristic.last())
            .or_else(|| cpu.last())
            .copied()
            .or_else(|| records.last())
            .cloned()
            .unwrap_or_else(|| json!({"agent":agent}));
        let chosen_set = if !native.is_empty() {
            &native
        } else if !heuristic.is_empty() {
            &heuristic
        } else {
            &cpu
        };
        let status = if chosen_set
            .iter()
            .any(|value| value.get("status").and_then(Value::as_str) == Some("input"))
        {
            "input".to_owned()
        } else if chosen_set
            .iter()
            .any(|value| value.get("status").and_then(Value::as_str) == Some("working"))
        {
            "working".to_owned()
        } else {
            chosen
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("idle")
                .to_owned()
        };
        let mut chosen = chosen;
        chosen["agent"] = json!(agent);
        chosen["active"] = json!(!active.is_empty());
        chosen["status"] = json!(status);
        if let Some(object) = chosen.as_object_mut() {
            for key in ["live", "ticks", "at", "idle", "file", "age"] {
                object.remove(key);
            }
        }
        result.insert(agent, chosen);
    }
    Value::Object(result)
}

fn sampled_idle(last: Option<&Value>, ticks: u64, now: i64) -> i64 {
    let elapsed = last
        .and_then(|value| value.get("at"))
        .and_then(Value::as_i64)
        .map(|at| now - at)
        .unwrap_or(0);
    let burnt = last
        .and_then(|value| value.get("ticks"))
        .and_then(Value::as_u64)
        .map(|old| ticks.saturating_sub(old))
        .unwrap_or(u64::MAX);
    let old_idle = last
        .and_then(|value| value.get("idle"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if elapsed >= 0 && burnt <= (2 * elapsed) as u64 {
        old_idle + elapsed
    } else {
        0
    }
}

fn sampled_harnesses() -> Vec<Value> {
    let samples = running_harnesses();
    let path = agent_dir().join(".cpu-sample.json");
    let previous = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    let now = epoch();
    let mut next = Map::new();
    let mut records = Vec::new();
    for (pid, agent, ticks) in samples {
        let last = previous.get(pid.to_string());
        let idle = sampled_idle(last, ticks, now);
        next.insert(pid.to_string(), json!({"ticks":ticks,"at":now,"idle":idle}));
        records.push(json!({"agent":agent,"pid":pid,"ticks":ticks,"live":true,"source":"cpu","status":if idle>=20{"input"}else{"working"},"updatedAt":timestamp()}));
    }
    let _ = atomic_write(
        &path,
        serde_json::to_string(&next).unwrap_or_default().as_bytes(),
    );
    records
}

fn running_harnesses() -> Vec<(u32, String, u64)> {
    let mut roots = HashMap::new();
    let mut processes = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        // stat already carries the same comm field as /proc/<pid>/comm.
        // Reuse that read for discovery as well as CPU accounting.
        let stat = fs::read_to_string(entry.path().join("stat")).ok();
        let parsed = stat.as_deref().and_then(parse_proc_stat);
        let mut name = if let Some((parent, ticks, name)) = parsed {
            processes.push((pid, parent, ticks));
            normalized_process_name(name).to_owned()
        } else {
            process_name(pid)
        };
        if !matches!(name.as_str(), "pi" | "opencode" | "codex" | "claude") {
            let bytes = fs::read(entry.path().join("cmdline")).unwrap_or_default();
            name =
                String::from_utf8_lossy(bytes.split(|byte| *byte == 0).next().unwrap_or_default())
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .trim_start_matches('.')
                    .trim_end_matches("-wrapped")
                    .to_owned();
        }
        if matches!(name.as_str(), "pi" | "opencode" | "codex" | "claude") {
            roots.insert(pid, name);
        }
    }
    let tree = ProcessTree::new(&processes);
    roots
        .into_iter()
        .map(|(pid, name)| (pid, name, tree.subtree_ticks(pid)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_snapshot_supplies_name_parent_and_ticks_together() {
        for name in [
            "pi",
            ".claude-wrapped",
            ".opencode-wrapp",
            "name with ) (parens)",
            "a\nb",
        ] {
            let text = format!("42 ({name}) S 7 0 0 0 0 0 0 0 0 0 13 5 0 0\n");
            assert_eq!(parse_proc_stat(&text), Some((7, 18, name)));
        }
        assert_eq!(normalized_process_name(".claude-wrapped\n"), "claude");
        assert_eq!(normalized_process_name(".opencode-wrapp"), "opencode");
        assert_eq!(normalized_process_name(" pi "), "pi");
    }

    #[test]
    fn malformed_stat_snapshots_are_ignored() {
        for text in [
            "",
            "42 no name S 7",
            "42 (pi) S 7",
            "42 (pi) S invalid",
            "42 (pi) S 7 0 0 0 0 0 0 0 0 0 nope 5",
            "42 (pi) S 7 0 0 0 0 0 0 0 0 0 18446744073709551615 1",
        ] {
            assert_eq!(parse_proc_stat(text), None);
        }
    }

    #[test]
    fn stat_name_matches_the_kernel_comm_for_this_process() {
        let pid = std::process::id();
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let (_, _, name) = parse_proc_stat(&stat).unwrap();
        assert_eq!(normalized_process_name(name), process_name(pid));
    }

    #[test]
    fn repeated_poll_in_one_second_preserves_idle_history() {
        let last = json!({"at":100,"ticks":42,"idle":20});
        assert_eq!(sampled_idle(Some(&last), 42, 100), 20);
        assert_eq!(sampled_idle(Some(&last), 42, 101), 21);
        assert_eq!(sampled_idle(Some(&last), 50, 100), 0);
        assert_eq!(sampled_idle(Some(&last), 42, 99), 0);
        assert_eq!(sampled_idle(None, 42, 100), 0);
    }
    #[test]
    fn one_snapshot_accounts_for_each_harness_and_its_descendants() {
        let processes = [(30, 20, 7), (20, 10, 5), (10, 1, 3), (40, 1, 11)];
        assert_eq!(subtree_ticks_in(10, &processes), 15);
        assert_eq!(subtree_ticks_in(20, &processes), 12);
        assert_eq!(subtree_ticks_in(40, &processes), 11);
    }

    #[test]
    fn process_tree_handles_missing_roots_cycles_and_reversed_chains() {
        assert_eq!(subtree_ticks_in(10, &[(20, 10, 5)]), 5);
        assert_eq!(subtree_ticks_in(10, &[(10, 20, 3), (20, 10, 5)]), 8);
        assert_eq!(subtree_ticks_in(99, &[(10, 1, 3)]), 0);
        assert_eq!(subtree_ticks_in(10, &[(10, 10, 3)]), 3);
        let chain: Vec<_> = (1..=10_000).rev().map(|pid| (pid, pid - 1, 1)).collect();
        assert_eq!(subtree_ticks_in(1, &chain), 10_000);
    }

    #[test]
    fn process_tree_matches_transitive_scan_for_reordered_snapshots() {
        // Independent reference: repeatedly expand a set, including duplicate
        // rows, orphaned parents and cycles from an inconsistent /proc snapshot.
        for seed in 0..100_u32 {
            let processes: Vec<_> = (0..40).map(|i| {
                ((i * 17 + seed) % 37, (i * 13 + seed * 3) % 43, u64::from(i))
            }).collect();
            let tree = ProcessTree::new(&processes);
            for root in 0..43 {
                let mut members = HashSet::from([root]);
                loop {
                    let before = members.len();
                    for &(pid, parent, _) in &processes {
                        if members.contains(&parent) { members.insert(pid); }
                    }
                    if members.len() == before { break; }
                }
                let expected: u64 = processes.iter().filter(|(pid, _, _)| members.contains(pid)).map(|(_, _, ticks)| ticks).sum();
                assert_eq!(tree.subtree_ticks(root), expected);
            }
        }
    }

}
