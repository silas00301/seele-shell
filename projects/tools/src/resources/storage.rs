//! Mounted local filesystems for the Resources panel's Storage group.
//!
//! The mount table comes from `/proc/self/mountinfo` and the sizes from
//! `statvfs(3)`. Only filesystems backed by a block device are listed, one row
//! per device: bind mounts, btrfs subvolumes and the read-only `/nix/store`
//! view share their device's row under its shortest mount point. Nothing is
//! opened below a mount point and nothing is written.

use serde::Serialize;
use std::ffi::CString;
use std::fs::File;
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MOUNTINFO_LIMIT: u64 = 1 << 20;
const MAX_MOUNTS: usize = 4096;
const MAX_FILESYSTEMS: usize = 32;
pub(super) const CADENCE: Duration = Duration::from_secs(5);
/// A reading older than this is reported as stale rather than current, which is
/// what a filesystem stalling inside `statvfs` looks like from the outside.
const STALE_AFTER: Duration = Duration::from_secs(15);
/// Image formats that are full by construction; a red bar for them is noise.
const IMAGES: [&str; 4] = ["squashfs", "erofs", "iso9660", "udf"];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Mount {
    source: String,
    fstype: String,
    point: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct Filesystem {
    id: String,
    mount: String,
    source: String,
    fstype: String,
    total: u64,
    used: u64,
    available: u64,
    read_only: bool,
}

/// mountinfo escapes space, tab, newline and backslash as three-digit octal.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let octal = bytes.get(index + 1..index + 4).filter(|digits| {
            bytes[index] == b'\\' && digits.iter().all(|digit| (b'0'..=b'7').contains(digit))
        });
        match octal.and_then(|digits| u8::from_str_radix(std::str::from_utf8(digits).ok()?, 8).ok())
        {
            Some(value) => {
                out.push(value);
                index += 4;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn clean(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .take(limit)
        .collect()
}

fn mounts(text: &str) -> Vec<Mount> {
    text.lines()
        .take(MAX_MOUNTS)
        .filter_map(|line| {
            let (local, remote) = line.split_once(" - ")?;
            let point = unescape(local.split(' ').nth(4)?);
            let mut remote = remote.split(' ');
            let fstype = remote.next()?.to_owned();
            let source = unescape(remote.next()?);
            Some(Mount {
                source,
                fstype,
                point,
            })
        })
        .collect()
}

/// One mount per block device, at its shortest mount point.
fn local(mounts: Vec<Mount>) -> Vec<Mount> {
    let mut chosen: Vec<Mount> = Vec::new();
    for mount in mounts {
        if !mount.source.starts_with("/dev/")
            || !mount.point.starts_with('/')
            || IMAGES.contains(&mount.fstype.as_str())
        {
            continue;
        }
        let shorter = |other: &Mount| {
            (mount.point.len(), mount.point.as_str()) < (other.point.len(), other.point.as_str())
        };
        match chosen
            .iter_mut()
            .find(|other| other.source == mount.source && other.fstype == mount.fstype)
        {
            Some(other) if shorter(other) => *other = mount,
            Some(_) => {}
            None => chosen.push(mount),
        }
    }
    chosen.sort_by(|a, b| a.point.cmp(&b.point));
    chosen
}

// `c_ulong` and `fsblkcnt_t` are 64 bits here but narrower on 32-bit targets.
#[allow(clippy::unnecessary_cast)]
fn measure(mount: &Mount) -> Option<Filesystem> {
    let path = CString::new(Path::new(&mount.point).as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is NUL-terminated and `stat` is writable for one statvfs.
    if unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: statvfs returned success, so it initialised the structure.
    let stat = unsafe { stat.assume_init() };
    let unit = if stat.f_frsize > 0 {
        stat.f_frsize
    } else {
        stat.f_bsize
    } as u64;
    let blocks = stat.f_blocks as u64;
    let free = stat.f_bfree as u64;
    let available = stat.f_bavail as u64;
    if unit == 0 || blocks == 0 || free > blocks || available > free {
        return None;
    }
    Some(Filesystem {
        id: format!("{}:{}", clean(&mount.source, 256), clean(&mount.fstype, 32)),
        mount: clean(&mount.point, 256),
        source: clean(&mount.source, 256),
        fstype: clean(&mount.fstype, 32),
        total: blocks.checked_mul(unit)?,
        used: (blocks - free).checked_mul(unit)?,
        available: available.checked_mul(unit)?,
        read_only: stat.f_flag & libc::ST_RDONLY != 0,
    })
}

fn read_bounded(path: &Path) -> io::Result<String> {
    let mut text = String::new();
    File::open(path)?
        .take(MOUNTINFO_LIMIT + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > MOUNTINFO_LIMIT {
        return Err(io::Error::other("mount table exceeds bound"));
    }
    Ok(text)
}

pub(super) fn sample(mountinfo: &Path) -> io::Result<Vec<Filesystem>> {
    Ok(local(mounts(&read_bounded(mountinfo)?))
        .iter()
        .filter_map(measure)
        .take(MAX_FILESYSTEMS)
        .collect())
}

type Reading = (Instant, io::Result<Vec<Filesystem>>);

/// The latest reading, shared between the sampler thread and the publisher.
#[derive(Clone, Default)]
pub(super) struct Latest(Arc<Mutex<Option<Reading>>>);

impl Latest {
    fn store(&self, reading: io::Result<Vec<Filesystem>>) {
        if let Ok(mut latest) = self.0.lock() {
            *latest = Some((Instant::now(), reading));
        }
    }
    pub(super) fn snapshot(&self, now: Instant) -> serde_json::Value {
        let Ok(latest) = self.0.lock() else {
            return serde_json::json!({"state":"unavailable","filesystems":[]});
        };
        match latest.as_ref() {
            None => serde_json::json!({"state":"pending","filesystems":[]}),
            Some((_, Err(_))) => serde_json::json!({"state":"unavailable","filesystems":[]}),
            Some((at, Ok(filesystems))) => serde_json::json!({
                "state": if now.saturating_duration_since(*at) > STALE_AFTER { "stale" } else { "current" },
                "filesystems": filesystems,
            }),
        }
    }
}

/// Sample on a thread of its own, so a filesystem that stalls inside `statvfs`
/// leaves the CPU and memory readings running and only this group goes stale.
pub(super) fn spawn(mountinfo: &'static str) -> Latest {
    let latest = Latest::default();
    let writer = latest.clone();
    let _ = std::thread::Builder::new()
        .name("storage".into())
        .spawn(move || loop {
            writer.store(sample(Path::new(mountinfo)));
            std::thread::sleep(CADENCE);
        });
    latest
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "\
22 1 259:2 / / rw,relatime shared:1 - ext4 /dev/nvme0n1p2 rw
23 22 259:2 /nix/store /nix/store ro,relatime shared:2 - ext4 /dev/nvme0n1p2 rw
24 22 259:1 / /boot rw,relatime shared:3 - vfat /dev/nvme0n1p1 rw,fmask=0022
25 22 0:23 / /proc rw,nosuid - proc proc rw
26 22 0:25 / /run rw,nosuid - tmpfs tmpfs rw,size=813148k
27 26 8:17 / /run/media/me/My\\040Stick rw,nosuid - exfat /dev/sdb1 rw
28 22 0:40 /@home /home rw,relatime - btrfs /dev/sda2 rw,subvol=/@home
29 22 0:41 /@ /srv rw,relatime - btrfs /dev/sda2 rw,subvol=/@
30 22 7:0 / /snap/core/1 ro - squashfs /dev/loop0 ro
31 22 0:50 / /mnt/share rw - nfs4 server:/export rw
32 22 8:33 / relative rw - ext4 /dev/sdc1 rw
broken line without separator";

    #[test]
    fn mountinfo_fields_and_octal_escapes() {
        let parsed = mounts(TABLE);
        assert_eq!(parsed.len(), 11);
        assert_eq!(parsed[5].point, "/run/media/me/My Stick");
        assert_eq!(parsed[5].fstype, "exfat");
        assert_eq!(unescape("a\\011b\\012c\\134d"), "a\tb\nc\\d");
        assert_eq!(unescape("trailing\\04"), "trailing\\04");
        assert_eq!(unescape("\\999"), "\\999");
    }

    #[test]
    fn one_row_per_device_at_its_shortest_mount() {
        let points: Vec<_> = local(mounts(TABLE)).into_iter().map(|m| m.point).collect();
        // /nix/store folds into /, both btrfs subvolumes into the shorter /srv,
        // and the pseudo, network, image and relative entries never appear.
        assert_eq!(points, ["/", "/boot", "/run/media/me/My Stick", "/srv"]);
    }

    #[test]
    fn a_real_statvfs_is_consistent_and_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let mount = Mount {
            source: "/dev/fixture".into(),
            fstype: "ext4".into(),
            point: dir.path().to_str().unwrap().into(),
        };
        let fs = measure(&mount).unwrap();
        assert!(fs.total > 0 && fs.used <= fs.total && fs.available <= fs.total);
        assert_eq!(fs.id, "/dev/fixture:ext4");
        let missing = Mount {
            point: dir.path().join("gone").to_str().unwrap().into(),
            ..mount
        };
        assert!(measure(&missing).is_none());
    }

    #[test]
    fn sample_reads_a_table_and_refuses_an_oversized_one() {
        let dir = tempfile::tempdir().unwrap();
        let table = dir.path().join("mountinfo");
        let point = dir.path().to_str().unwrap().replace(' ', "\\040");
        std::fs::write(
            &table,
            format!(
                "1 1 8:1 / {point} rw - ext4 /dev/fixture rw\n2 1 0:1 / /proc rw - proc proc rw\n"
            ),
        )
        .unwrap();
        let found = sample(&table).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mount, dir.path().to_str().unwrap());
        std::fs::write(&table, vec![b'x'; MOUNTINFO_LIMIT as usize + 1]).unwrap();
        assert!(sample(&table).is_err());
        assert!(sample(&dir.path().join("absent")).is_err());
    }

    #[test]
    fn display_strings_lose_controls_and_direction_marks() {
        assert_eq!(clean("/run/media/a\u{202e}b\u{7}c", 64), "/run/media/abc");
        assert_eq!(clean("abcdef", 3), "abc");
    }

    #[test]
    fn snapshot_states_pending_current_stale_and_unavailable() {
        let latest = Latest::default();
        let now = Instant::now();
        assert_eq!(latest.snapshot(now)["state"], "pending");
        latest.store(Ok(vec![]));
        assert_eq!(latest.snapshot(Instant::now())["state"], "current");
        assert_eq!(
            latest.snapshot(Instant::now() + STALE_AFTER + Duration::from_secs(1))["state"],
            "stale"
        );
        latest.store(Err(io::Error::other("fixture")));
        assert_eq!(latest.snapshot(Instant::now())["state"], "unavailable");
    }
}
