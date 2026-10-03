//! Fixture system trees laid out the way NixOS builds a generation: top-level
//! links into store paths named `<32 hash characters>-<name>`, a buildEnv
//! module tree, a firmware tree, and the generated kernel-params and
//! switch-inhibitors files. `/run/booted-system` and `/run/current-system`
//! are links into that fake store.
use seele_maintenance::{
    model::{self, Finding, Inbox, Lifecycle, Result, Urgency},
    restart,
    service::{action_args, Broker, Collector, Service},
    Executor,
};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::{atomic::AtomicUsize, Arc, Mutex},
    time::Duration,
};

#[derive(Clone)]
struct Spec {
    kernel: &'static str,
    /// Bumping this rebuilds the kernel at the same version.
    kernel_build: u32,
    nvidia: Option<&'static str>,
    /// NVIDIA's open modules, as on nerv; they also bring GSP firmware.
    open: bool,
    initrd: u32,
    params: &'static str,
    firmware: &'static str,
    systemd: &'static str,
    systemd_build: u32,
    bus: &'static str,
    implementation: &'static str,
    /// Anything else a switch applies in place; a new value is a new generation.
    userland: u32,
}
fn base() -> Spec {
    Spec {
        kernel: "6.12.8",
        kernel_build: 0,
        nvidia: Some("570.153.02"),
        open: true,
        initrd: 0,
        params: "loglevel=4 quiet root=UUID=0b6e1c2a-feed nvidia-drm.modeset=1",
        firmware: "20250808",
        systemd: "257.5",
        systemd_build: 0,
        bus: "dbus-broker-36",
        implementation: "broker",
        userland: 0,
    }
}
struct Store {
    _dir: tempfile::TempDir,
    store: PathBuf,
    booted: PathBuf,
    current: PathBuf,
}
fn hash(seed: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(seed.as_bytes()))[..32].to_owned()
}
impl Store {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("nix/store");
        fs::create_dir_all(&store).unwrap();
        fs::create_dir_all(dir.path().join("run")).unwrap();
        Self {
            booted: dir.path().join("run/booted-system"),
            current: dir.path().join("run/current-system"),
            store,
            _dir: dir,
        }
    }
    /// One store path per (name, inputs), so a rebuild keeps its name and gets
    /// a new hash, exactly as Nix does.
    fn path(&self, name: &str, inputs: impl std::fmt::Display) -> PathBuf {
        let path = self
            .store
            .join(format!("{}-{name}", hash(&format!("{name}#{inputs}"))));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn file(&self, path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if !path.exists() {
            fs::write(path, text).unwrap();
        }
    }
    fn link(&self, target: &Path, link: &Path) {
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        let _ = fs::remove_file(link);
        symlink(target, link).unwrap();
    }
    fn system(&self, spec: &Spec) -> PathBuf {
        let s = spec;
        let identity = format!(
            "{}-{}-{:?}-{}-{}-{}-{}-{}-{}-{}-{}-{}",
            s.kernel,
            s.kernel_build,
            s.nvidia,
            s.open,
            s.initrd,
            s.params,
            s.firmware,
            s.systemd,
            s.systemd_build,
            s.bus,
            s.implementation,
            s.userland
        );
        let root = self
            .store
            .join(format!("{}-nixos-system-nerv-25.11", hash(&identity)));
        if root.exists() {
            return root;
        }
        fs::create_dir_all(&root).unwrap();
        let kernel_name = format!("linux-{}", s.kernel);
        let kernel = self.path(&kernel_name, s.kernel_build);
        self.file(&kernel.join("bzImage"), "kernel");
        self.link(&kernel.join("bzImage"), &root.join("kernel"));

        // The kernel's own modules output and the aggregated tree, which
        // nixpkgs names after the kernel too.
        let own = self.path(&format!("{kernel_name}-modules"), s.kernel_build);
        let own_kernel = own.join(format!("lib/modules/{}/kernel", s.kernel));
        self.file(&own_kernel.join("drivers/gpu/amdgpu.ko.xz"), "module");
        let tree = self.path(
            &format!("{kernel_name}-modules"),
            format!("tree#{}#{:?}#{}", s.kernel_build, s.nvidia, s.open),
        );
        let release = tree.join(format!("lib/modules/{}", s.kernel));
        fs::create_dir_all(&release).unwrap();
        self.file(&release.join("modules.dep"), "");
        self.link(&own_kernel, &release.join("kernel"));
        // nixpkgs' nvidia-x11/kernel-modules.nix: pname nvidia-open or
        // nvidia-kernel-modules, version "<driver>-<kernel>", installed by
        // kbuild's modules_install under updates/.
        if let Some(nvidia) = s.nvidia {
            let pname = if s.open {
                "nvidia-open"
            } else {
                "nvidia-kernel-modules"
            };
            let driver = self.path(&format!("{pname}-{nvidia}-{}", s.kernel), 0);
            let updates = driver.join(format!("lib/modules/{}/updates", s.kernel));
            self.file(&updates.join("nvidia.ko.xz"), "module");
            self.link(&updates, &release.join("updates"));
        }
        self.link(&tree, &root.join("kernel-modules"));

        let initrd = self.path(
            &format!("initrd-{kernel_name}"),
            format!("{}#{}", s.initrd, s.kernel_build),
        );
        self.file(&initrd.join("initrd"), "initrd");
        self.link(&initrd.join("initrd"), &root.join("initrd"));
        fs::write(root.join("kernel-params"), s.params).unwrap();

        let upstream = self.path(&format!("linux-firmware-{}-zstd", s.firmware), 0);
        self.file(&upstream.join("lib/firmware/amdgpu/psp.bin.zst"), "blob");
        let gsp = s.nvidia.filter(|_| s.open);
        let firmware = self.path("firmware", format!("{}#{gsp:?}", s.firmware));
        let tree = firmware.join("lib/firmware");
        fs::create_dir_all(&tree).unwrap();
        self.link(&upstream.join("lib/firmware/amdgpu"), &tree.join("amdgpu"));
        // hardware.nvidia.gsp: the driver's uncompressed `firmware` output.
        if let Some(nvidia) = gsp {
            let output = self.path(&format!("nvidia-x11-{nvidia}-firmware"), 0);
            let files = output.join(format!("lib/firmware/nvidia/{nvidia}"));
            self.file(&files.join("gsp_ga10x.bin"), "blob");
            self.link(&output.join("lib/firmware/nvidia"), &tree.join("nvidia"));
        }
        self.link(&tree, &root.join("firmware"));

        let systemd = self.path(&format!("systemd-{}", s.systemd), s.systemd_build);
        self.link(&systemd, &root.join("systemd"));
        let bus = self.path(s.bus, 0);
        let binary = if s.bus.starts_with("dbus-broker") {
            "dbus-broker"
        } else {
            "dbus-daemon"
        };
        self.file(&bus.join("bin").join(binary), "bus");
        self.link(
            &bus.join("bin").join(binary),
            &root.join("sw/bin").join(binary),
        );
        let inhibitors = self.path("switch-inhibitors", s.implementation);
        self.file(
            &inhibitors.join("inhibitors.json"),
            &json!({ "dbus-implementation": s.implementation }).to_string(),
        );
        self.link(
            &inhibitors.join("inhibitors.json"),
            &root.join("switch-inhibitors"),
        );
        root
    }
    fn boot(&self, spec: &Spec) {
        let root = self.system(spec);
        self.link(&root, &self.booted);
        self.link(&root, &self.current);
    }
    fn switch(&self, spec: &Spec) {
        let root = self.system(spec);
        self.link(&root, &self.current);
    }
    fn report(&self) -> Result<Option<Finding>> {
        restart::report(&self.booted, &self.current)
    }
    fn changes(&self) -> Vec<String> {
        restart::changes(&self.booted, &self.current).unwrap()
    }
}

#[test]
fn store_names_and_versions_follow_nix() {
    let path = Path::new("/nix/store/0123456789abcdefghijklmnopqrstuv-systemd-257.6/lib");
    assert_eq!(restart::store_name(path).unwrap(), "systemd-257.6");
    assert!(restart::store_name(Path::new("/nix/store/short-systemd")).is_none());
    assert!(
        restart::store_name(Path::new("/nix/store/0123456789ABCDEFGHIJKLMNOPQRSTUV-x")).is_none()
    );
    assert_eq!(restart::split_name("systemd-257.6"), ("systemd", "257.6"));
    assert_eq!(
        restart::split_name("nvidia-open-575.64-6.12.10"),
        ("nvidia-open", "575.64-6.12.10")
    );
    assert_eq!(restart::split_name("firmware"), ("firmware", ""));
}

#[test]
fn the_booted_generation_and_a_userland_switch_need_nothing() {
    let store = Store::new();
    store.boot(&base());
    assert!(store.report().unwrap().is_none());
    // A new generation that changed only what a switch applies in place.
    store.switch(&Spec {
        userland: 1,
        ..base()
    });
    assert!(store.changes().is_empty());
    assert!(store.report().unwrap().is_none());
}

#[test]
fn a_kernel_update_names_versions_and_folds_what_it_implies() {
    let store = Store::new();
    store.boot(&base());
    store.switch(&Spec {
        kernel: "6.12.10",
        nvidia: Some("575.64"),
        firmware: "20250911",
        ..base()
    });
    assert_eq!(
        store.changes(),
        [
            "Linux 6.12.8 → 6.12.10",
            "kernel modules (nvidia-open 570.153.02 → 575.64)",
            "device firmware (linux-firmware 20250808 → 20250911, nvidia-x11 570.153.02 → 575.64)",
        ]
    );
    let finding = store.report().unwrap().unwrap();
    assert_eq!(finding.key, restart::KEY);
    assert_eq!(
        finding.title,
        "Restart to apply Linux 6.12.8 → 6.12.10 and 2 more changes"
    );
    assert_eq!(
        finding.details,
        "Linux 6.12.8 → 6.12.10\nKernel modules (nvidia-open 570.153.02 → 575.64)\nDevice firmware (linux-firmware 20250808 → 20250911, nvidia-x11 570.153.02 → 575.64)"
    );
    assert_eq!(finding.urgency, Urgency::Eventually);
    assert_eq!(finding.lifecycle, Lifecycle::Ongoing);
    assert_eq!(finding.actions, ["open-power", "recheck"]);
    // The kernel rebuilt the initrd as well; that is implied, not listed.
    assert!(!finding.details.contains("initrd"));
}

#[test]
fn a_driver_rebuild_and_an_initrd_change_are_named_on_their_own() {
    let store = Store::new();
    store.boot(&base());
    store.switch(&Spec {
        nvidia: Some("575.64"),
        ..base()
    });
    assert_eq!(
        store.changes(),
        [
            "kernel modules (nvidia-open 570.153.02 → 575.64)",
            "device firmware (nvidia-x11 570.153.02 → 575.64)",
        ]
    );
    // The closed modules carry their own name and no GSP firmware.
    store.switch(&Spec {
        nvidia: Some("575.64"),
        open: false,
        ..base()
    });
    assert_eq!(
        store.changes(),
        [
            "kernel modules (nvidia-kernel-modules 575.64 added, nvidia-open removed)",
            "device firmware (nvidia-x11 removed)",
        ]
    );
    store.boot(&Spec {
        open: false,
        ..base()
    });
    store.switch(&Spec {
        nvidia: Some("575.64"),
        open: false,
        ..base()
    });
    assert_eq!(
        store.changes(),
        ["kernel modules (nvidia-kernel-modules 570.153.02 → 575.64)"]
    );
    store.boot(&base());
    store.switch(&Spec {
        nvidia: None,
        ..base()
    });
    assert_eq!(
        store.changes(),
        [
            "kernel modules (nvidia-open removed)",
            "device firmware (nvidia-x11 removed)",
        ]
    );
    store.switch(&Spec {
        initrd: 1,
        ..base()
    });
    let finding = store.report().unwrap().unwrap();
    assert_eq!(finding.title, "Restart to apply early boot image (initrd)");
    assert_eq!(finding.details, "Early boot image (initrd)");
    store.switch(&Spec {
        kernel_build: 1,
        ..base()
    });
    assert_eq!(store.changes()[0], "Linux 6.12.8, rebuilt");
}

#[test]
fn kernel_parameters_are_named_without_their_values() {
    let store = Store::new();
    store.boot(&base());
    store.switch(&Spec {
        params: "loglevel=7 root=UUID=7d1f9a33-beef nvidia-drm.modeset=1 mitigations=auto",
        ..base()
    });
    let changes = store.changes();
    assert_eq!(
        changes,
        ["kernel command line (loglevel changed, mitigations added, quiet removed, root changed)"]
    );
    let finding = store.report().unwrap().unwrap();
    for value in ["0b6e1c2a", "7d1f9a33", "UUID", "=7"] {
        assert!(!finding.details.contains(value));
        assert!(!finding.title.contains(value));
    }
}

#[test]
fn the_login_manager_and_bus_keep_their_booted_builds() {
    let store = Store::new();
    store.boot(&base());
    store.switch(&Spec {
        systemd: "257.6",
        bus: "dbus-broker-37",
        ..base()
    });
    assert_eq!(
        store.changes(),
        [
            "login manager (systemd 257.5 → 257.6)",
            "message bus (dbus-broker 36 → 37)",
        ]
    );
    store.switch(&Spec {
        systemd_build: 1,
        ..base()
    });
    assert_eq!(store.changes(), ["login manager (systemd 257.5, rebuilt)"]);
    // Switching implementation is a declared switch inhibitor upstream.
    store.switch(&Spec {
        bus: "dbus-1.14.10",
        implementation: "dbus",
        ..base()
    });
    assert_eq!(
        store.changes(),
        [
            "message bus (dbus-broker 36 → dbus 1.14.10)",
            "dbus-implementation (broker → dbus)",
        ]
    );
}

#[test]
fn flavoured_releases_come_from_the_module_tree() {
    let store = Store::new();
    store.boot(&Spec {
        nvidia: None,
        ..base()
    });
    store.switch(&Spec {
        kernel: "6.12.10-zen1",
        nvidia: None,
        ..base()
    });
    assert_eq!(store.changes(), ["Linux 6.12.8 → 6.12.10-zen1"]);
}

#[test]
fn an_unreadable_generation_fails_the_probe_instead_of_resolving() {
    let store = Store::new();
    store.boot(&base());
    fs::remove_file(&store.booted).unwrap();
    assert_eq!(store.report().unwrap_err(), "probe_unavailable");
    store.link(&store.store.join("missing"), &store.booted);
    assert!(store.report().is_err());
    // A malformed generated file is not silently read as "no change".
    store.boot(&base());
    let broken = store.system(&Spec {
        userland: 9,
        ..base()
    });
    fs::remove_file(broken.join("switch-inhibitors")).unwrap();
    fs::write(broken.join("switch-inhibitors"), "[1]").unwrap();
    store.switch(&Spec {
        userland: 9,
        ..base()
    });
    assert_eq!(store.report().unwrap_err(), "invalid_probe_snapshot");
}

#[test]
fn reports_are_stable_so_unchanged_generations_do_not_advance_revisions() {
    let store = Store::new();
    store.boot(&base());
    store.switch(&Spec {
        kernel: "6.12.10",
        ..base()
    });
    let first = store.report().unwrap().unwrap();
    let mut inbox = Inbox::new(model::registrations(&json!({})), None, 1.0).unwrap();
    let (row, notify, changed) = inbox.publish("restart", first, 1.0).unwrap();
    assert!(changed);
    assert!(!notify, "a pending restart never raises a notification");
    let (again, notify, changed) = inbox
        .publish("restart", store.report().unwrap().unwrap(), 2.0)
        .unwrap();
    assert!(!changed && !notify);
    assert_eq!(again.revision, row.revision);
    // The registry offers exactly one way out, and it only opens a panel.
    let actions = &inbox.registrations["restart"];
    assert!(!actions["open-power"].disruptive);
    assert_eq!(actions["open-power"].label, "Open Power");
    assert_eq!(
        action_args(&json!({}), &row, "open-power").unwrap(),
        ["seele-shellctl", "power"]
    );
    let (disk, _, _) = inbox
        .publish(
            "disk",
            serde_json::from_value(
                json!({"key":"root","title":"Disk","urgency":"soon","actions":["recheck"]}),
            )
            .unwrap(),
            1.0,
        )
        .unwrap();
    assert!(action_args(&json!({}), &disk, "open-power").is_err());
    assert!(inbox
        .publish(
            "disk",
            serde_json::from_value(
                json!({"key":"root","title":"Disk","urgency":"soon","actions":["open-power"]})
            )
            .unwrap(),
            1.0,
        )
        .is_err());
}

#[derive(Default)]
struct Execute {
    calls: Mutex<Vec<Vec<String>>>,
}
impl Executor for Execute {
    fn run(&self, arguments: &[String], _: &[u8], _: Duration) -> Result<(i32, String)> {
        self.calls.lock().unwrap().push(arguments.to_vec());
        Ok((0, String::new()))
    }
}
struct NoBroker;
impl Broker for NoBroker {
    fn call(&self, _: &Value, _: Duration, _: bool) -> Result<Value> {
        panic!("broker called without explicit analysis")
    }
}

#[test]
fn the_service_publishes_once_and_resolves_after_reboot_or_rollback() {
    let store = Arc::new(Store::new());
    store.boot(&base());
    let probe = store.clone();
    let collector: Arc<Collector> = Arc::new(move |_, source, _, _| {
        if source == "restart" {
            Ok(probe.report()?.into_iter().collect())
        } else {
            Ok(vec![])
        }
    });
    let config = json!({});
    let execute = Arc::new(Execute::default());
    let service = Arc::new(Service::new(
        config.clone(),
        Inbox::new(model::registrations(&config), None, model::now()).unwrap(),
        Arc::new(AtomicUsize::new(0)),
        execute.clone(),
        Arc::new(NoBroker),
        collector,
    ));
    let rows = |part: &str| service.call(&json!({"op":"list"})).unwrap()[part].clone();

    service.check("restart").unwrap();
    assert_eq!(rows("active"), json!([]));
    let updated = Spec {
        kernel: "6.12.10",
        ..base()
    };
    store.switch(&updated);
    for _ in 0..3 {
        service.check("restart").unwrap();
    }
    let active = rows("active");
    assert_eq!(active.as_array().unwrap().len(), 1);
    assert_eq!(active[0]["id"], "restart:booted-system");
    assert_eq!(
        active[0]["revision"], 1,
        "rechecks deduplicate by source and key"
    );
    assert_eq!(
        active[0]["actions"][0],
        json!({"id":"open-power","label":"Open Power","disruptive":false})
    );
    assert_eq!(
        service.call(&json!({"op":"list"})).unwrap()["urgency"],
        "eventually"
    );
    assert!(
        execute.calls.lock().unwrap().is_empty(),
        "no notification is sent"
    );

    // Open Power runs the fixed argv and nothing else; it restarts nothing.
    service
        .call(&json!({"op":"action","id":active[0]["id"],"revision":1,"action":"open-power"}))
        .unwrap();
    service.wait_jobs();
    assert_eq!(
        *execute.calls.lock().unwrap(),
        [vec!["seele-shellctl".to_owned(), "power".to_owned()]]
    );

    // A failed probe keeps the finding rather than presenting it as resolved.
    fs::remove_file(&store.current).unwrap();
    assert!(service.check("restart").is_err());
    assert_eq!(rows("active").as_array().unwrap().len(), 1);

    // Rolling back makes the booted generation current again.
    store.switch(&base());
    service.check("restart").unwrap();
    assert_eq!(rows("active"), json!([]));
    assert_eq!(rows("history")[0]["id"], "restart:booted-system");

    // Switching forward again and then booting into it resolves it as well.
    store.switch(&updated);
    service.check("restart").unwrap();
    assert_eq!(rows("active")[0]["recurrence"], 1);
    store.boot(&updated);
    service.check("restart").unwrap();
    assert_eq!(rows("active"), json!([]));
    service.close();
}
