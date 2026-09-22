use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

const HISTORY: usize = 60;
const MAX_PROCESSES: usize = 32768;
const MAX_ROWS: usize = 256;
const CADENCE: Duration = Duration::from_secs(1);

fn read(path: impl AsRef<Path>, limit: u64) -> io::Result<String> {
    let mut value = String::new();
    File::open(path)?
        .take(limit + 1)
        .read_to_string(&mut value)?;
    if value.len() as u64 > limit {
        return Err(io::Error::other("reading exceeds bound"));
    }
    Ok(value)
}
#[derive(Clone, Debug)]
struct Cpu {
    total: u64,
    idle: u64,
    cores: u32,
    counters: Vec<u64>,
}
fn cpu(text: &str) -> Option<Cpu> {
    let line = text.lines().next()?;
    let mut parts = line.split_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }
    let fields = parts
        .take(8)
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if fields.len() < 4 {
        return None;
    }
    Some(Cpu {
        total: fields.iter().try_fold(0u64, |sum, n| sum.checked_add(*n))?,
        idle: fields[3].checked_add(*fields.get(4).unwrap_or(&0))?,
        counters: fields.clone(),
        cores: text
            .lines()
            .filter(|l| {
                l.strip_prefix("cpu")
                    .is_some_and(|s| s.starts_with(|c: char| c.is_ascii_digit()))
            })
            .count()
            .try_into()
            .ok()?,
    })
}
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Memory {
    total: u64,
    available: u64,
    used: u64,
    swap_total: u64,
    swap_used: u64,
}
fn memory(text: &str) -> Option<Memory> {
    let values: HashMap<_, _> = text
        .lines()
        .filter_map(|line| {
            let mut p = line.split_whitespace();
            let key = p.next()?.trim_end_matches(':');
            let value = p.next()?.parse::<u64>().ok()?.checked_mul(1024)?;
            (p.next()? == "kB").then_some((key, value))
        })
        .collect();
    let total = *values.get("MemTotal")?;
    let available = *values.get("MemAvailable")?;
    let swap_total = *values.get("SwapTotal")?;
    let swap_free = *values.get("SwapFree")?;
    if total == 0 || available > total || swap_free > swap_total {
        return None;
    }
    Some(Memory {
        total,
        available,
        used: total - available,
        swap_total,
        swap_used: swap_total - swap_free,
    })
}
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Process {
    id: String,
    pid: u32,
    name: String,
    state: String,
    threads: u64,
    rss: u64,
    virtual_bytes: u64,
    cpu: Option<f64>,
    #[serde(skip)]
    ticks: u64,
}
fn process(text: &str, page_size: u64) -> Option<Process> {
    let (pid, rest) = text.split_once(" (")?;
    let (name, rest) = rest.rsplit_once(") ")?;
    let fields: Vec<_> = rest.split_whitespace().collect();
    let number = |i: usize| fields.get(i)?.parse::<u64>().ok();
    let pid = pid.parse::<u32>().ok()?;
    let start = number(19)?;
    Some(Process {
        id: format!("{pid}:{start}"),
        pid,
        name: name
            .chars()
            .filter(|c| {
                !c.is_control() && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            })
            .take(64)
            .collect(),
        state: match *fields.first()? {
            "R" => "Running",
            "S" => "Sleeping",
            "D" => "Disk wait",
            "T" | "t" => "Stopped",
            "Z" => "Zombie",
            "I" => "Idle",
            _ => "Unknown",
        }
        .into(),
        ticks: number(11)?.checked_add(number(12)?)?,
        threads: number(17)?,
        virtual_bytes: number(20)?,
        rss: number(21)?.checked_mul(page_size)?,
        cpu: None,
    })
}
struct Sample {
    cpu: Option<Cpu>,
    memory: Option<Memory>,
    processes: Vec<Process>,
    limited: bool,
}
fn sample(root: &Path, page_size: u64) -> Sample {
    let cpu = read(root.join("stat"), 131072).ok().and_then(|s| cpu(&s));
    let memory = read(root.join("meminfo"), 32768)
        .ok()
        .and_then(|s| memory(&s));
    let mut processes = Vec::new();
    let mut limited = false;
    if let Ok(entries) = fs::read_dir(root) {
        let mut count = 0;
        for (index, entry) in entries.take(65537).enumerate() {
            if index == 65536 {
                limited = true;
                break;
            }
            let Ok(entry) = entry else {
                limited = true;
                continue;
            };
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.is_empty() || !name.bytes().all(|c| c.is_ascii_digit()) {
                continue;
            }
            count += 1;
            if count > MAX_PROCESSES {
                limited = true;
                break;
            }
            match read(entry.path().join("stat"), 8192)
                .ok()
                .and_then(|s| process(&s, page_size))
            {
                Some(p) if p.pid.to_string() == name => processes.push(p),
                _ => limited = true,
            }
        }
    } else {
        limited = true;
    }
    Sample {
        cpu,
        memory,
        processes,
        limited,
    }
}
#[derive(Default)]
struct State {
    previous: Option<Cpu>,
    previous_processes: HashMap<String, u64>,
    cpu_history: VecDeque<Option<f64>>,
    memory_history: VecDeque<Option<f64>>,
    processes: Vec<Process>,
    memory: Option<Memory>,
    usage: Option<f64>,
    limited: bool,
    cores: u32,
    query: String,
    sort: String,
    selected: String,
}
fn append(values: &mut VecDeque<Option<f64>>, value: Option<f64>) {
    if values.len() == HISTORY {
        values.pop_front();
    }
    values.push_back(value);
}
impl State {
    fn gap(&mut self, elapsed: Duration) {
        if elapsed < Duration::from_secs(2) {
            return;
        }
        self.previous = None;
        self.previous_processes.clear();
        for _ in 0..elapsed.as_secs().saturating_sub(1).min(HISTORY as u64) {
            append(&mut self.cpu_history, None);
            append(&mut self.memory_history, None);
        }
    }
    fn update(&mut self, sample: Sample) {
        let delta = sample
            .cpu
            .as_ref()
            .zip(self.previous.as_ref())
            .and_then(|(now, old)| {
                if now.counters.len() != old.counters.len()
                    || now.counters.iter().zip(&old.counters).any(|(a, b)| a < b)
                {
                    return None;
                }
                let total = now.total.checked_sub(old.total)?;
                let idle = now.idle.checked_sub(old.idle)?;
                (now.cores == old.cores && now.cores > 0 && total > 0 && idle <= total)
                    .then_some((total, idle))
            });
        self.cores = sample.cpu.as_ref().map_or(0, |c| c.cores);
        self.usage = delta.map(|(total, idle)| 100.0 * (total - idle) as f64 / total as f64);
        self.processes = sample.processes;
        for p in &mut self.processes {
            p.cpu = delta.and_then(|(total, _)| {
                let ticks = p.ticks.checked_sub(*self.previous_processes.get(&p.id)?)?;
                Some(
                    (100.0 * ticks as f64 * self.cores as f64 / total as f64)
                        .min(100.0 * self.cores as f64),
                )
            });
        }
        self.previous_processes = self
            .processes
            .iter()
            .map(|p| (p.id.clone(), p.ticks))
            .collect();
        self.previous = sample.cpu;
        self.memory = sample.memory;
        self.limited = sample.limited;
        append(&mut self.cpu_history, self.usage);
        append(
            &mut self.memory_history,
            self.memory
                .as_ref()
                .map(|m| 100.0 * m.used as f64 / m.total as f64),
        );
    }
    fn request(&mut self, value: Value) {
        match value["op"].as_str() {
            Some("query") => {
                self.query = value["text"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(128)
                    .collect::<String>()
                    .to_lowercase()
            }
            Some("sort") => {
                self.sort = if value["value"] == "memory" {
                    "memory"
                } else {
                    "cpu"
                }
                .into()
            }
            Some("select") => {
                self.selected = value["id"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(64)
                    .collect()
            }
            _ => (),
        }
    }
    fn snapshot(&self) -> Value {
        let mut rows: Vec<_> = self
            .processes
            .iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&self.query)
                    || p.pid.to_string().contains(&self.query)
            })
            .collect();
        rows.sort_by(|a, b| {
            let order = if self.sort == "memory" {
                b.rss.cmp(&a.rss)
            } else {
                b.cpu
                    .partial_cmp(&a.cpu)
                    .unwrap_or(std::cmp::Ordering::Equal)
            };
            order.then_with(|| a.pid.cmp(&b.pid))
        });
        let matched = rows.len();
        rows.truncate(MAX_ROWS);
        let selected = self.processes.iter().find(|p| p.id == self.selected);
        json!({"version":1,"type":"snapshot","cadenceSeconds":CADENCE.as_secs(),"historyCapacity":HISTORY,"cpu":self.usage,"cores":self.cores,"memory":self.memory,"cpuHistory":self.cpu_history,"memoryHistory":self.memory_history,"rows":rows,"total":self.processes.len(),"matched":matched,"limited":self.limited,"selected":selected,"selectionGone": !self.selected.is_empty() && selected.is_none(),"sort":if self.sort == "memory" {"memory"} else {"cpu"},"query":self.query})
    }
}
fn publish(state: &State) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, &state.snapshot())?;
    out.write_all(b"\n")?;
    out.flush()
}
pub fn run() -> io::Result<()> {
    if std::env::args().len() != 1 {
        return Err(io::Error::other("usage: seele-resources"));
    }
    // One process owns one open panel; closing stdin or killing the worker drops
    // all histories. Requests only alter the projection, never sampling cadence.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size <= 0 {
        return Err(io::Error::other("page size unavailable"));
    }
    let mut state = State::default();
    let mut pending = Vec::new();
    let mut next = Instant::now();
    let mut last_sample = None;
    loop {
        if Instant::now() >= next {
            let sampled = SystemTime::now();
            if let Some(previous) = last_sample {
                state.gap(
                    sampled
                        .duration_since(previous)
                        .unwrap_or(Duration::from_secs(HISTORY as u64)),
                );
            }
            last_sample = Some(sampled);
            state.update(sample(Path::new("/proc"), page_size as u64));
            publish(&state)?;
            next = Instant::now() + CADENCE;
        }
        let timeout = next
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(1000) as i32;
        let mut fd = libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut fd, 1, timeout.max(1)) };
        if ready < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(io::Error::last_os_error());
        }
        if ready == 0 {
            continue;
        }
        let mut bytes = [0u8; 4096];
        let count = io::stdin().read(&mut bytes)?;
        if count == 0 {
            return Ok(());
        }
        for byte in &bytes[..count] {
            if *byte == b'\n' {
                if let Ok(request) = serde_json::from_slice(&pending) {
                    state.request(request);
                }
                pending.clear();
            } else if pending.len() < 4096 {
                pending.push(*byte);
            } else {
                return Err(io::Error::other("request exceeds bound"));
            }
        }
        publish(&state)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn proc(pid: u32, start: u64, ticks: u64) -> Process {
        let mut f = vec!["0".to_string(); 22];
        f[0] = "S".into();
        f[11] = ticks.to_string();
        f[17] = "3".into();
        f[19] = start.to_string();
        f[20] = "16384".into();
        f[21] = "2".into();
        process(
            &format!("{pid} (name (with) brackets) {}", f.join(" ")),
            4096,
        )
        .unwrap()
    }
    fn observation(total: u64, idle: u64, processes: Vec<Process>) -> Sample {
        Sample {
            cpu: Some(Cpu {
                total,
                idle,
                cores: 4,
                counters: vec![total, idle],
            }),
            memory: None,
            processes,
            limited: false,
        }
    }
    #[test]
    fn stat_fields_and_no_command_line() {
        let p = proc(12, 100, 20);
        assert_eq!(p.name, "name (with) brackets");
        assert_eq!(p.rss, 8192);
        assert_eq!(p.threads, 3);
        assert_eq!(p.id, "12:100");
        assert!(process("broken", 4096).is_none());
    }
    #[test]
    fn memory_uses_available_not_free() {
        let m = memory("MemTotal: 100 kB\nMemAvailable: 30 kB\nMemFree: 5 kB\nSwapTotal: 20 kB\nSwapFree: 12 kB").unwrap();
        assert_eq!(m.used, 70 * 1024);
        assert_eq!(m.swap_used, 8 * 1024);
        assert!(
            memory("MemTotal: 1 kB\nMemAvailable: 2 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB").is_none()
        );
    }
    #[test]
    fn aggregate_excludes_guest_double_count() {
        let c = cpu("cpu 10 20 30 40 5 6 7 8 999 999\ncpu0 1\ncpu1 1").unwrap();
        assert_eq!(c.total, 126);
        assert_eq!(c.idle, 45);
        assert_eq!(c.cores, 2);
    }
    #[test]
    fn first_sample_reuse_reset_and_disappearance() {
        let mut s = State::default();
        s.update(observation(100, 20, vec![proc(1, 1, 10)]));
        assert!(s.usage.is_none());
        assert!(s.processes[0].cpu.is_none());
        s.update(observation(200, 70, vec![proc(1, 1, 35)]));
        assert_eq!(s.usage, Some(50.0));
        assert_eq!(s.processes[0].cpu, Some(100.0));
        s.selected = "1:1".into();
        s.update(observation(300, 90, vec![proc(1, 2, 100)]));
        assert!(s.processes[0].cpu.is_none());
        assert_eq!(s.snapshot()["selectionGone"], true);
        s.update(observation(10, 2, vec![proc(1, 2, 2)]));
        assert!(s.usage.is_none());
        assert!(s.processes[0].cpu.is_none());
        s.update(observation(110, 30, vec![]));
        assert!(s.previous_processes.is_empty());
    }
    #[test]
    fn counter_reset_hidden_by_another_counter_and_hotplug() {
        let mut s = State::default();
        let mut initial = observation(100, 50, vec![]);
        initial.cpu = cpu("cpu 40 0 10 50 0 0 0 0\ncpu0 1");
        s.update(initial);
        let mut next = observation(110, 100, vec![]);
        next.cpu = cpu("cpu 0 0 10 100 0 0 0 0\ncpu0 1");
        s.update(next);
        assert!(s.usage.is_none());
        s.update(observation(500, 200, vec![]));
        assert!(s.usage.is_none());
    }
    #[test]
    fn missed_samples_break_history_and_rebaseline() {
        let mut s = State::default();
        s.update(observation(100, 20, vec![proc(1, 1, 10)]));
        s.gap(Duration::from_secs(8));
        s.update(observation(900, 100, vec![proc(1, 1, 50)]));
        assert_eq!(s.cpu_history.len(), 9);
        assert!(s.cpu_history.iter().all(Option::is_none));
        assert!(s.processes[0].cpu.is_none());
        s.gap(Duration::from_secs(100000));
        assert_eq!(s.cpu_history.len(), HISTORY);
    }
    #[test]
    fn synthetic_proc_tree_bounds_and_disappearance() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("stat"), "cpu 10 0 5 85 0 0 0 0\ncpu0 1\n").unwrap();
        fs::write(
            root.path().join("meminfo"),
            "MemTotal: 100 kB\nMemAvailable: 40 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB",
        )
        .unwrap();
        fs::create_dir(root.path().join("42")).unwrap();
        fs::write(root.path().join("42/stat"), "not a stat file").unwrap();
        let result = sample(root.path(), 4096);
        assert!(result.limited);
        assert!(result.processes.is_empty());
        assert_eq!(result.memory.unwrap().used, 60 * 1024);
        fs::remove_dir_all(root.path().join("42")).unwrap();
        assert!(!sample(root.path(), 4096).limited);
        fs::write(root.path().join("oversize"), vec![b'x'; 12]).unwrap();
        assert!(read(root.path().join("oversize"), 10).is_err());
    }
    #[test]
    fn bounded_history_search_sort_and_detail() {
        let mut s = State::default();
        for i in 0..90 {
            s.update(observation(100 + i, i, vec![proc(2, 1, i)]));
        }
        assert_eq!(s.cpu_history.len(), 60);
        s.request(json!({"op":"query","text":"BRACKETS"}));
        assert_eq!(s.snapshot()["matched"], 1);
        s.request(json!({"op":"query","text":"missing"}));
        assert_eq!(s.snapshot()["matched"], 0);
        s.request(json!({"op":"select","id":"2:1"}));
        assert_eq!(s.snapshot()["selected"]["pid"], 2);
        s.processes = (1..300).map(|i| proc(i, 1, 0)).collect();
        s.query.clear();
        s.processes[50].rss = 99999;
        s.sort = "memory".into();
        let snap = s.snapshot();
        assert_eq!(snap["cadenceSeconds"], 1);
        assert_eq!(snap["historyCapacity"], 60);
        assert_eq!(snap["rows"].as_array().unwrap().len(), 256);
        assert_eq!(snap["rows"][0]["pid"], 51);
    }
}
