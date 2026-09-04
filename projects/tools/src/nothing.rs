// Nothing's 0x55 protocol framing and commands are adapted from Something X:
// https://github.com/SoaOaoS/something-x (MIT, Copyright 2026 SoaOaoS).

use crate::command::{atomic_write, runtime_home};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::mem;
use std::os::fd::RawFd;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const AF_BLUETOOTH: libc::c_int = 31;
const BTPROTO_RFCOMM: libc::c_int = 3;
const CTRL_HOST_CRC: u16 = 0x0160;
const CMD_PROTOCOL: u16 = 0xC001;
const CMD_ACTIVATE: u16 = 0xF001;
const CMD_BATTERY: u16 = 0xC007;
const CMD_NOISE: u16 = 0xC01E;
const CMD_SET_NOISE: u16 = 0xF00F;
const EVENT_BATTERY: u16 = 0xE001;
const EVENT_NOISE: u16 = 0xE003;
const CHANNEL_PRIORITY: [u8; 3] = [15, 17, 16];

#[repr(C)]
struct SockAddrRc {
    family: libc::sa_family_t,
    address: [u8; 6],
    channel: u8,
}

struct BluetoothSocket(RawFd);

impl BluetoothSocket {
    fn connect(address: &str, channel: u8) -> Result<Self> {
        let bytes = bluetooth_address(address).ok_or("invalid Bluetooth address")?;
        let fd = unsafe { libc::socket(AF_BLUETOOTH, libc::SOCK_STREAM, BTPROTO_RFCOMM) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let socket = Self(fd);
        socket.set_timeout(Duration::from_secs(2))?;
        let target = SockAddrRc {
            family: AF_BLUETOOTH as libc::sa_family_t,
            address: bytes,
            channel,
        };
        let result = unsafe {
            libc::connect(
                fd,
                &target as *const SockAddrRc as *const libc::sockaddr,
                mem::size_of::<SockAddrRc>() as libc::socklen_t,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        socket.set_timeout(Duration::from_millis(250))?;
        Ok(socket)
    }

    fn set_timeout(&self, duration: Duration) -> Result {
        let value = libc::timeval {
            tv_sec: duration.as_secs() as libc::time_t,
            tv_usec: duration.subsec_micros() as libc::suseconds_t,
        };
        for option in [libc::SO_RCVTIMEO, libc::SO_SNDTIMEO] {
            let result = unsafe {
                libc::setsockopt(
                    self.0,
                    libc::SOL_SOCKET,
                    option,
                    &value as *const libc::timeval as *const libc::c_void,
                    mem::size_of::<libc::timeval>() as libc::socklen_t,
                )
            };
            if result != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        Ok(())
    }
}

impl Read for BluetoothSocket {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = unsafe {
            libc::read(
                self.0,
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
            )
        };
        if count < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(count as usize)
        }
    }
}

impl Write for BluetoothSocket {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let count =
            unsafe { libc::write(self.0, buffer.as_ptr() as *const libc::c_void, buffer.len()) };
        if count < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(count as usize)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for BluetoothSocket {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.0);
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadphoneState {
    pub address: String,
    pub battery: Option<u8>,
    pub controls: bool,
    pub noise_mode: String,
    pub updated_at: u64,
}

#[derive(Deserialize, Serialize)]
struct HeadphoneCommand {
    address: String,
    mode: String,
}

fn directory() -> PathBuf {
    runtime_home().join("seele-shell")
}

pub fn state_path() -> PathBuf {
    directory().join("nothing-headphones.json")
}

pub fn command_path() -> PathBuf {
    directory().join("nothing-headphones.command")
}

pub fn state(address: &str) -> Option<HeadphoneState> {
    let path = state_path();
    let fresh =
        fs::metadata(&path).ok()?.modified().ok()?.elapsed().ok()? < Duration::from_secs(45);
    let value = serde_json::from_str::<HeadphoneState>(&fs::read_to_string(path).ok()?).ok()?;
    (fresh && value.address.eq_ignore_ascii_case(address)).then_some(value)
}

pub fn queue_noise(address: &str, mode: &str) -> Result {
    if !matches!(mode, "off" | "anc" | "transparency" | "adaptive") {
        return Err("invalid Nothing headphone noise mode".into());
    }
    let command = HeadphoneCommand {
        address: address.to_owned(),
        mode: mode.to_owned(),
    };
    atomic_write(&command_path(), &serde_json::to_vec(&command)?)?;
    if std::env::var("SEELE_NOTHING_HEADPHONES_DISABLE_DAEMON").as_deref() == Ok("1") {
        if let Some(mut value) = state(address) {
            value.noise_mode = mode.to_owned();
            value.updated_at = epoch();
            atomic_write(&state_path(), &serde_json::to_vec(&value)?)?;
        }
        return Ok(());
    }
    for _ in 0..20 {
        if !command_path().exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = fs::remove_file(command_path());
    Err("Nothing headphone controls did not respond".into())
}

fn bluetooth_address(address: &str) -> Option<[u8; 6]> {
    let parsed = address
        .split(':')
        .map(|part| u8::from_str_radix(part, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    (parsed.len() == 6).then(|| {
        let mut bytes = [0_u8; 6];
        for (index, value) in parsed.into_iter().rev().enumerate() {
            bytes[index] = value;
        }
        bytes
    })
}

fn channels(address: &str) -> Vec<u8> {
    let discovered = Command::new("sdptool")
        .args(["browse", address])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Channel:")?.trim().parse().ok())
        .collect::<Vec<u8>>();
    let mut seen = HashSet::new();
    let priority = CHANNEL_PRIORITY
        .into_iter()
        .filter(|channel| discovered.contains(channel))
        .collect::<Vec<_>>();
    priority
        .into_iter()
        .chain(discovered)
        .chain(CHANNEL_PRIORITY)
        .filter(|channel| seen.insert(*channel))
        .collect()
}

fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0xFFFF_u16;
    for byte in data {
        crc ^= u16::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

fn frame(command: u16, payload: &[u8], sequence: &mut u8) -> Vec<u8> {
    *sequence = sequence.wrapping_add(1);
    let mut value = vec![0x55];
    value.extend_from_slice(&CTRL_HOST_CRC.to_le_bytes());
    value.extend_from_slice(&command.to_le_bytes());
    value.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    value.push(*sequence);
    value.extend_from_slice(payload);
    value.extend_from_slice(&crc16(&value).to_le_bytes());
    value
}

fn messages(buffer: &mut Vec<u8>) -> Vec<(u16, Vec<u8>)> {
    let mut result = Vec::new();
    loop {
        let Some(start) = buffer.iter().position(|byte| *byte == 0x55) else {
            buffer.clear();
            break;
        };
        if start > 0 {
            buffer.drain(..start);
        }
        if buffer.len() < 8 {
            break;
        }
        let control = u16::from_le_bytes([buffer[1], buffer[2]]);
        let length = u16::from_le_bytes([buffer[5], buffer[6]]) as usize;
        let crc_length = usize::from(control & 0x20 != 0) * 2;
        let total = 8 + length + crc_length;
        if buffer.len() < total {
            break;
        }
        let raw = buffer.drain(..total).collect::<Vec<_>>();
        if crc_length == 2 {
            let received = u16::from_le_bytes([raw[8 + length], raw[9 + length]]);
            if received != crc16(&raw[..8 + length]) {
                continue;
            }
        }
        let command = u16::from_le_bytes([raw[3], raw[4]]) | 0x8000;
        result.push((command, raw[8..8 + length].to_vec()));
    }
    result
}

fn connect(address: &str, sequence: &mut u8) -> Result<(BluetoothSocket, Vec<u8>)> {
    for channel in channels(address) {
        let Ok(mut socket) = BluetoothSocket::connect(address, channel) else {
            continue;
        };
        socket.write_all(&frame(CMD_PROTOCOL, &[], sequence))?;
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut initial = Vec::new();
        let mut chunk = [0_u8; 256];
        while Instant::now() < deadline {
            match socket.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => {
                    initial.extend_from_slice(&chunk[..count]);
                    if initial.contains(&0x55) {
                        return Ok((socket, initial));
                    }
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
        }
    }
    Err("Nothing headphone protocol is unavailable".into())
}

fn parse_battery(payload: &[u8]) -> Option<u8> {
    let count = *payload.first()? as usize;
    (0..count).find_map(|index| {
        let offset = 1 + index * 2;
        (payload.get(offset) == Some(&6))
            .then(|| payload.get(offset + 1).map(|value| value & 0x7F))?
    })
}

fn parse_noise(payload: &[u8]) -> Option<&'static str> {
    payload
        .chunks_exact(3)
        .find(|entry| entry[0] == 1)
        .map(|entry| match entry[1] {
            5 | 0 => "off",
            7 => "transparency",
            4 => "adaptive",
            _ => "anc",
        })
}

fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn save(state: &mut HeadphoneState) -> Result {
    state.updated_at = epoch();
    atomic_write(&state_path(), &serde_json::to_vec(state)?)
}

fn send(socket: &mut BluetoothSocket, sequence: &mut u8, command: u16, payload: &[u8]) -> Result {
    socket.write_all(&frame(command, payload, sequence))?;
    Ok(())
}

fn session(address: &str, running: &AtomicBool) -> Result {
    let mut sequence = 0;
    let (mut socket, mut buffer) = connect(address, &mut sequence)?;
    let mut state = HeadphoneState {
        address: address.to_owned(),
        battery: None,
        controls: false,
        noise_mode: String::new(),
        updated_at: epoch(),
    };
    save(&mut state)?;
    let mut activated = false;
    let mut activation_sent = None;
    let mut queried = Instant::now();
    let mut chunk = [0_u8; 512];
    while running.load(Ordering::SeqCst) {
        let mut changed = false;
        for (command, payload) in messages(&mut buffer) {
            match command {
                CMD_PROTOCOL => {
                    send(&mut socket, &mut sequence, CMD_ACTIVATE, &[])?;
                    activation_sent = Some(Instant::now());
                }
                CMD_ACTIVATE => {
                    activated = true;
                    state.controls = true;
                    send(&mut socket, &mut sequence, CMD_BATTERY, &[])?;
                    send(&mut socket, &mut sequence, CMD_NOISE, &[3])?;
                    queried = Instant::now();
                    changed = true;
                }
                CMD_BATTERY | EVENT_BATTERY => {
                    if let Some(battery) = parse_battery(&payload) {
                        state.battery = Some(battery);
                        changed = true;
                    }
                }
                CMD_NOISE | EVENT_NOISE => {
                    if let Some(mode) = parse_noise(&payload) {
                        state.noise_mode = mode.to_owned();
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        if !activated
            && activation_sent.is_some_and(|started| started.elapsed() >= Duration::from_secs(2))
        {
            activated = true;
            state.controls = true;
            send(&mut socket, &mut sequence, CMD_BATTERY, &[])?;
            send(&mut socket, &mut sequence, CMD_NOISE, &[3])?;
            queried = Instant::now();
            changed = true;
        }
        if activated && queried.elapsed() >= Duration::from_secs(15) {
            send(&mut socket, &mut sequence, CMD_BATTERY, &[])?;
            send(&mut socket, &mut sequence, CMD_NOISE, &[3])?;
            queried = Instant::now();
        }
        if activated && command_path().exists() {
            let command = fs::read_to_string(command_path())
                .ok()
                .and_then(|value| serde_json::from_str::<HeadphoneCommand>(&value).ok());
            let _ = fs::remove_file(command_path());
            if let Some(command) =
                command.filter(|value| value.address.eq_ignore_ascii_case(address))
            {
                let value = match command.mode.as_str() {
                    "off" => 5,
                    "transparency" => 7,
                    "adaptive" => 4,
                    _ => 1,
                };
                send(&mut socket, &mut sequence, CMD_SET_NOISE, &[1, value, 0])?;
                state.noise_mode = command.mode;
                changed = true;
            }
        }
        if changed {
            save(&mut state)?;
        }
        match socket.read(&mut chunk) {
            Ok(0) => return Err("Nothing headphone disconnected".into()),
            Ok(count) => buffer.extend_from_slice(&chunk[..count]),
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub fn run(arguments: &[String]) -> Result {
    let address = arguments.first().ok_or("Bluetooth address required")?;
    if bluetooth_address(address).is_none() {
        return Err("invalid Bluetooth address".into());
    }
    fs::create_dir_all(directory())?;
    let _ = fs::remove_file(command_path());
    let running = Arc::new(AtomicBool::new(true));
    let signal = running.clone();
    ctrlc::set_handler(move || signal.store(false, Ordering::SeqCst))?;
    while running.load(Ordering::SeqCst) {
        if let Err(error) = session(address, &running) {
            eprintln!("{error}");
            if let Some(mut state) = state(address) {
                state.controls = false;
                save(&mut state)?;
            }
        }
        if running.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_secs(3));
        }
    }
    let _ = fs::remove_file(state_path());
    let _ = fs::remove_file(command_path());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_addresses_use_bluez_byte_order() {
        assert_eq!(
            bluetooth_address("11:22:33:44:55:66"),
            Some([0x66, 0x55, 0x44, 0x33, 0x22, 0x11])
        );
        assert_eq!(bluetooth_address("not-an-address"), None);
    }

    #[test]
    fn frames_include_crc_and_round_trip() {
        let mut sequence = 0;
        let mut value = frame(CMD_NOISE, &[3], &mut sequence);
        // Device responses clear bit 15.
        value[4] &= 0x7F;
        let checksum = crc16(&value[..value.len() - 2]).to_le_bytes();
        let length = value.len();
        value[length - 2..].copy_from_slice(&checksum);
        assert_eq!(messages(&mut value), vec![(CMD_NOISE, vec![3])]);
        assert!(value.is_empty());
    }

    #[test]
    fn headphone_payloads_report_battery_and_noise() {
        assert_eq!(parse_battery(&[1, 6, 0x80 | 74]), Some(74));
        assert_eq!(parse_noise(&[1, 5, 0, 2, 4, 0]), Some("off"));
        assert_eq!(parse_noise(&[1, 7, 0, 2, 4, 0]), Some("transparency"));
        assert_eq!(parse_noise(&[1, 4, 0, 2, 4, 0]), Some("adaptive"));
        assert_eq!(parse_noise(&[1, 1, 0, 2, 4, 0]), Some("anc"));
    }
}
