//! Local kernel byte counters only. One process is one open-panel session.
use crate::Result;
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    io::{self, BufRead, Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

const CAPACITY: usize = 60;
const LIMIT: usize = 256;
const INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
struct Sample {
    index: u32,
    inode: u64,
    name: String,
    state: String,
    counters: Option<(u64, u64)>,
}
fn small(path: &Path) -> Option<String> {
    let mut value = String::new();
    fs::File::open(path)
        .ok()?
        .take(65)
        .read_to_string(&mut value)
        .ok()?;
    (value.len() <= 64).then(|| value.trim().to_owned())
}
fn collect(root: &Path) -> io::Result<(Vec<Sample>, bool)> {
    let mut entries = fs::read_dir(root)?
        .filter_map(|entry| entry.ok())
        .take(LIMIT + 1)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    let limited = entries.len() > LIMIT;
    entries.truncate(LIMIT);
    let samples = entries
        .into_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().into_string().ok()?;
            if name.is_empty() || name.len() > 15 || name.chars().any(char::is_control) {
                return None;
            }
            let index = small(&path.join("ifindex"))?.parse::<u32>().ok()?;
            if index == 0 {
                return None;
            }
            let inode = fs::metadata(&path).ok()?.ino();
            let state = match small(&path.join("operstate")).as_deref() {
                Some("up") => "Up",
                Some("down") => "Down",
                Some("dormant") => "Dormant",
                Some("lowerlayerdown") => "Lower layer down",
                Some("notpresent") => "Not present",
                Some("testing") => "Testing",
                _ => "Unknown",
            }
            .to_owned();
            let counters = small(&path.join("statistics/rx_bytes")).and_then(|rx| {
                Some((
                    rx.parse().ok()?,
                    small(&path.join("statistics/tx_bytes"))?.parse().ok()?,
                ))
            });
            // Recheck identity after the multi-file read: hotplug cannot join two devices.
            if small(&path.join("ifindex"))?.parse::<u32>().ok()? != index
                || fs::metadata(&path).ok()?.ino() != inode
            {
                return None;
            }
            Some(Sample {
                index,
                inode,
                name,
                state,
                counters,
            })
        })
        .collect();
    Ok((samples, limited))
}
#[derive(Default)]
struct Track {
    inode: u64,
    previous: Option<(u64, u64, Duration)>,
    rx_total: u64,
    tx_total: u64,
    incomplete: bool,
    rx: VecDeque<Option<f64>>,
    tx: VecDeque<Option<f64>>,
}
fn push(history: &mut VecDeque<Option<f64>>, value: Option<f64>) {
    if history.len() == CAPACITY {
        history.pop_front();
    }
    history.push_back(value);
}
fn bytes(value: f64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut scaled = value;
    let mut unit = 0;
    while scaled >= 1024.0 && unit < units.len() - 1 {
        scaled /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{scaled:.0} {}", units[unit])
    } else {
        format!("{scaled:.1} {}", units[unit])
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    name: String,
    state: String,
    status: &'static str,
    rx_rate: Option<f64>,
    tx_rate: Option<f64>,
    rx_label: String,
    tx_label: String,
    rx_total: String,
    tx_total: String,
    incomplete: bool,
    rx: Vec<Option<f64>>,
    tx: Vec<Option<f64>>,
    maximum: f64,
    scale_label: String,
}
#[derive(Default)]
struct Session {
    tracks: BTreeMap<u32, Track>,
    last: Option<Duration>,
}
impl Session {
    fn sample(&mut self, samples: Vec<Sample>, now: Duration) -> Vec<Row> {
        self.tracks
            .retain(|index, _| samples.iter().any(|s| s.index == *index));
        let skipped = self.last.map_or(0, |last| {
            now.saturating_sub(last)
                .as_secs()
                .saturating_sub(1)
                .min(CAPACITY as u64) as usize
        });
        self.last = Some(now);
        samples
            .into_iter()
            .map(|sample| {
                let track = self.tracks.entry(sample.index).or_default();
                if track.inode != sample.inode {
                    *track = Track {
                        inode: sample.inode,
                        ..Track::default()
                    };
                }
                for _ in 0..skipped {
                    push(&mut track.rx, None);
                    push(&mut track.tx, None);
                }
                let mut rx_rate = None;
                let mut tx_rate = None;
                let mut status = "First sample · waiting for a rate";
                if let Some((rx, tx)) = sample.counters {
                    if let Some((old_rx, old_tx, old_time)) = track.previous {
                        let elapsed = now.saturating_sub(old_time).as_secs_f64();
                        if let (Some(rx_delta), Some(tx_delta)) =
                            (rx.checked_sub(old_rx), tx.checked_sub(old_tx))
                        {
                            track.rx_total = track.rx_total.saturating_add(rx_delta);
                            track.tx_total = track.tx_total.saturating_add(tx_delta);
                            if elapsed > 0.0 && elapsed <= 3.0 {
                                rx_rate = Some(rx_delta as f64 / elapsed);
                                tx_rate = Some(tx_delta as f64 / elapsed);
                                status = "Live · sampled every second";
                            } else {
                                status = "Sampling gap · waiting for the next rate";
                            }
                        } else {
                            track.incomplete = true;
                            status = "Counters restarted · totals contain observed bytes only";
                        }
                    }
                    track.previous = Some((rx, tx, now));
                } else {
                    track.previous = None;
                    track.incomplete = true;
                    status = "Counters unavailable";
                }
                push(&mut track.rx, rx_rate);
                push(&mut track.tx, tx_rate);
                let peak = track
                    .rx
                    .iter()
                    .chain(track.tx.iter())
                    .flatten()
                    .copied()
                    .fold(0.0, f64::max);
                let maximum = if peak <= 1024.0 {
                    1024.0
                } else {
                    2_f64.powf(peak.log2().ceil())
                };
                Row {
                    id: format!("{}:{}", sample.index, sample.inode),
                    name: sample.name,
                    state: sample.state,
                    status,
                    rx_rate,
                    tx_rate,
                    rx_label: rx_rate.map_or_else(|| "—".into(), |v| format!("{}/s", bytes(v))),
                    tx_label: tx_rate.map_or_else(|| "—".into(), |v| format!("{}/s", bytes(v))),
                    rx_total: bytes(track.rx_total as f64),
                    tx_total: bytes(track.tx_total as f64),
                    incomplete: track.incomplete,
                    rx: track.rx.iter().copied().collect(),
                    tx: track.tx.iter().copied().collect(),
                    maximum,
                    scale_label: format!("{}/s", bytes(maximum)),
                }
            })
            .collect()
    }
    fn unavailable(&mut self) {
        // Cannot safely subtract across an unreadable discovery pass.
        for track in self.tracks.values_mut() {
            track.previous = None;
            track.incomplete = true;
        }
    }
}
fn emit(session: &mut Session, root: &Path, now: Duration) -> Result {
    let value = match collect(root) {
        Ok((samples, limited)) => {
            serde_json::json!({"version":1,"rows":session.sample(samples, now),"limited":limited,"error":"","elapsed":now.as_secs(),"interfaceLimit":LIMIT,"historyCapacity":CAPACITY})
        }
        Err(_) => {
            session.unavailable();
            serde_json::json!({"version":1,"rows":[],"limited":false,"error":"Interface counters are unavailable","elapsed":now.as_secs(),"interfaceLimit":LIMIT,"historyCapacity":CAPACITY})
        }
    };
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, &value)?;
    writeln!(out)?;
    out.flush()?;
    Ok(())
}
pub fn run(arguments: &[String]) -> Result {
    if !arguments.is_empty() {
        return Err("usage: seele-network-activity (JSON lines on stdin: reset)".into());
    }
    let root = std::env::var_os("SEELE_NETWORK_ACTIVITY_SYSFS")
        .map_or_else(|| PathBuf::from("/sys/class/net"), PathBuf::from);
    let (sender, receiver) = mpsc::sync_channel(4);
    std::thread::spawn(move || {
        let mut input = io::stdin().lock();
        loop {
            let mut line = Vec::new();
            match input.by_ref().take(1025).read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) if line.len() > 1024 => break,
                Ok(_) => {
                    if serde_json::from_slice::<serde_json::Value>(&line)
                        .ok()
                        .as_ref()
                        .and_then(|v| v.get("op"))
                        .and_then(|v| v.as_str())
                        == Some("reset")
                        && sender.send(()).is_err()
                    {
                        break;
                    }
                }
            }
        }
    });
    let mut session = Session::default();
    let mut start = Instant::now();
    emit(&mut session, &root, start.elapsed())?;
    let mut next = Instant::now() + INTERVAL;
    loop {
        match receiver.recv_timeout(next.saturating_duration_since(Instant::now())) {
            Ok(()) => {
                session = Session::default();
                start = Instant::now();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
        emit(&mut session, &root, start.elapsed())?;
        next = Instant::now() + INTERVAL;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(index: u32, inode: u64, rx: u64, tx: u64) -> Sample {
        Sample {
            index,
            inode,
            name: "eth0".into(),
            state: "Up".into(),
            counters: Some((rx, tx)),
        }
    }
    #[test]
    fn discovery_bounds_and_validates_kernel_fields() {
        let root = tempfile::tempdir().unwrap();
        for index in 1..=LIMIT + 3 {
            let path = root.path().join(format!("v{index}"));
            fs::create_dir_all(path.join("statistics")).unwrap();
            fs::write(path.join("ifindex"), index.to_string()).unwrap();
            fs::write(path.join("operstate"), "hostile/unrecognized").unwrap();
            fs::write(
                path.join("statistics/rx_bytes"),
                "999999999999999999999999999999999999999999999999999999999999999999999999",
            )
            .unwrap();
            fs::write(path.join("statistics/tx_bytes"), "-1").unwrap();
        }
        let (samples, limited) = collect(root.path()).unwrap();
        assert!(limited);
        assert_eq!(samples.len(), LIMIT);
        assert!(samples
            .iter()
            .all(|sample| sample.state == "Unknown" && sample.counters.is_none()));
    }
    #[test]
    fn counters_above_javascript_precision_subtract_as_integers() {
        let mut s = Session::default();
        let base = 1_u64 << 60;
        s.sample(vec![item(2, 1, base, base)], Duration::ZERO);
        let row = s.sample(vec![item(2, 1, base + 1, base + 3)], Duration::from_secs(1));
        assert_eq!(row[0].rx_rate, Some(1.0));
        assert_eq!(row[0].tx_rate, Some(3.0));
        assert_eq!(row[0].rx_total, "1 B");
    }
    #[test]
    fn rates_use_elapsed_and_first_sample_is_unknown() {
        let mut s = Session::default();
        let first = s.sample(vec![item(2, 1, 1000, 100)], Duration::ZERO);
        assert_eq!(first[0].rx_rate, None);
        assert_eq!(first[0].rx_total, "0 B");
        let rows = s.sample(vec![item(2, 1, 4000, 700)], Duration::from_millis(1500));
        assert_eq!(rows[0].rx_rate, Some(2000.0));
        assert_eq!(rows[0].tx_rate, Some(400.0));
        assert_eq!(rows[0].rx_total, "2.9 KiB");
    }
    #[test]
    fn gaps_and_reset_never_spike() {
        let mut s = Session::default();
        s.sample(vec![item(2, 1, 1000, 100)], Duration::ZERO);
        let gap = s.sample(vec![item(2, 1, 5000, 900)], Duration::from_secs(10));
        assert_eq!(gap[0].rx_rate, None);
        assert_eq!(gap[0].rx.len(), 11);
        assert_eq!(gap[0].rx_total, "3.9 KiB");
        let reset = s.sample(vec![item(2, 1, 10, 20)], Duration::from_secs(11));
        assert!(reset[0].incomplete);
        assert_eq!(reset[0].rx_rate, None);
        let after = s.sample(vec![item(2, 1, 110, 70)], Duration::from_secs(12));
        assert_eq!(after[0].rx_rate, Some(100.0));
    }
    #[test]
    fn hotplug_reuse_and_rename() {
        let mut s = Session::default();
        s.sample(vec![item(2, 1, 1000, 100)], Duration::ZERO);
        let mut renamed = item(2, 1, 2000, 200);
        renamed.name = "renamed".into();
        assert_eq!(
            s.sample(vec![renamed], Duration::from_secs(1))[0].rx_rate,
            Some(1000.0)
        );
        let replacement = s.sample(vec![item(2, 8, 999999, 9)], Duration::from_secs(2));
        assert_eq!(replacement[0].rx_rate, None);
        assert_eq!(replacement[0].rx_total, "0 B");
        s.sample(vec![], Duration::from_secs(3));
        assert!(s.tracks.is_empty());
        assert_eq!(
            s.sample(vec![item(2, 8, 1111111, 10)], Duration::from_secs(4))[0].rx_rate,
            None
        );
    }
    #[test]
    fn unreadable_counters_and_history_bounds() {
        let mut s = Session::default();
        s.sample(vec![item(2, 1, 1000, 100)], Duration::ZERO);
        let mut missing = item(2, 1, 2000, 200);
        missing.counters = None;
        let rows = s.sample(vec![missing], Duration::from_secs(1));
        assert!(rows[0].incomplete);
        assert_eq!(rows[0].rx_rate, None);
        assert_eq!(
            s.sample(vec![item(2, 1, 9000, 900)], Duration::from_secs(2))[0].rx_rate,
            None
        );
        for t in 3..100 {
            s.sample(vec![item(2, 1, 9000 + t, 900 + t)], Duration::from_secs(t));
        }
        assert_eq!(s.tracks[&2].rx.len(), CAPACITY);
        s.unavailable();
        assert_eq!(
            s.sample(vec![item(2, 1, 99999, 99999)], Duration::from_secs(101))[0].rx_rate,
            None
        );
    }
}
