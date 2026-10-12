use std::{
    fs::{self, File, OpenOptions, Permissions},
    io::{ErrorKind, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};
use clap::Subcommand;
use const_format::concatcp;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{android::susfs::api::features::show, assets, defs};

const CONFIG_PATH: &str = concatcp!(defs::WORKING_DIR, ".susfs-lkm.json");
const STATUS_PATH: &str = concatcp!(defs::LOG_DIR, "susfs-lkm-status.json");
const LOCK_PATH: &str = concatcp!(defs::WORKING_DIR, ".susfs-lkm.lock");
const METADATA_ASSET: &str = "susfs-lkm.json";
const MODULE_ASSET: &str = "susfs_guard_lkm-android14-6.1.ko";
const SUPPORTED_KMI: &str = "android14-6.1";
const REQUIRED_FEATURES: &[&str] = &[
    "SUSFS_LKM_MODULES",
    "CONFIG_KSU_SUSFS_SUS_PATH",
    "CONFIG_KSU_SUSFS_SUS_MOUNT",
    "CONFIG_KSU_SUSFS_SUS_KSTAT",
    "CONFIG_KSU_SUSFS_SPOOF_UNAME",
    "CONFIG_KSU_SUSFS_ENABLE_LOG",
    "CONFIG_KSU_SUSFS_HIDE_KSU_SUSFS_SYMBOLS",
    "CONFIG_KSU_SUSFS_SPOOF_CMDLINE_OR_BOOTCONFIG",
    "CONFIG_KSU_SUSFS_OPEN_REDIRECT",
    "CONFIG_KSU_SUSFS_SUS_MAP",
];

#[derive(Debug, Subcommand)]
pub enum LkmCommand {
    Status,
    Enable,
    Disable,
}

#[derive(Default, Deserialize, Serialize)]
struct Config {
    enabled: bool,
}

#[derive(Deserialize)]
struct Metadata {
    version: String,
    source_commit: String,
    kmi: String,
    file: String,
    sha256: String,
}

#[derive(Deserialize, Serialize)]
pub struct Status {
    enabled: bool,
    supported: bool,
    available: bool,
    version: String,
    kmi: String,
    state: String,
    message: String,
}

#[derive(Deserialize, Serialize)]
struct BootStatus {
    boot_id: String,
    #[serde(flatten)]
    status: Status,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(data) => Ok(Some(
            serde_json::from_slice(&data).with_context(|| format!("Invalid {path}"))?,
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Cannot read {path}")),
    }
}

fn save_json(path: &str, value: &impl Serialize) -> Result<()> {
    let target = Path::new(path);
    let parent = target.parent().context("Missing configuration directory")?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary
        .as_file()
        .set_permissions(Permissions::from_mode(0o600))?;
    serde_json::to_writer_pretty(&mut temporary, value)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(target)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn lock() -> Result<File> {
    fs::create_dir_all(defs::WORKING_DIR)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(LOCK_PATH)?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(std::io::Error::last_os_error()).context("SUSFS LKM loader is busy");
    }
    Ok(lock)
}

fn boot_id() -> Result<String> {
    let id = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    ensure!(!id.trim().is_empty(), "Kernel boot ID is empty");
    Ok(id.trim().to_owned())
}

fn current_kmi() -> Result<String> {
    let release = fs::read_to_string("/proc/sys/kernel/osrelease")?;
    let pattern = regex_lite::Regex::new(r"^(\d+\.\d+)\.\d+-android(\d+)(?:-|$)")?;
    let parts = pattern
        .captures(release.trim())
        .context("Cannot identify the running kernel KMI")?;
    Ok(format!("android{}-{}", &parts[2], &parts[1]))
}

fn metadata() -> Result<Metadata> {
    let metadata: Metadata = serde_json::from_slice(&assets::get_asset(METADATA_ASSET)?)?;
    ensure!(
        metadata.kmi == SUPPORTED_KMI && metadata.file == MODULE_ASSET,
        "Unexpected SUSFS LKM payload"
    );
    ensure!(
        metadata.source_commit.len() == 40
            && metadata
                .source_commit
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()),
        "Invalid SUSFS source commit"
    );
    let data = assets::get_asset(MODULE_ASSET)?;
    ensure!(
        sha256::digest(data.as_ref()) == metadata.sha256,
        "SUSFS LKM payload checksum mismatch"
    );
    ensure!(
        data.len() >= 64
            && data.starts_with(b"\x7fELF")
            && data[4] == 2
            && data[5] == 1
            && u16::from_le_bytes([data[16], data[17]]) == 1
            && u16::from_le_bytes([data[18], data[19]]) == 183,
        "Expected an AArch64 ELF64 relocatable SUSFS module"
    );
    Ok(metadata)
}

fn initial_status(config: &Config) -> Status {
    let kmi = current_kmi().unwrap_or_default();
    let payload = metadata();
    let supported = cfg!(target_arch = "aarch64") && kmi == SUPPORTED_KMI;
    let available = payload.is_ok();
    let version = payload
        .as_ref()
        .map(|metadata| metadata.version.clone())
        .unwrap_or_default();
    let (state, message) = if !supported {
        (
            "unsupported",
            "Native SUSFS LKM requires AArch64 android14-6.1".to_owned(),
        )
    } else if let Err(error) = payload {
        ("unavailable", format!("{error:#}"))
    } else if config.enabled {
        (
            "pending",
            "SUSFS LKM will be loaded at the next normal boot".to_owned(),
        )
    } else {
        (
            "disabled",
            "Native SUSFS LKM autoload is disabled".to_owned(),
        )
    };
    Status {
        enabled: config.enabled,
        supported,
        available,
        version,
        kmi,
        state: state.to_owned(),
        message,
    }
}

fn module_present() -> Result<bool> {
    if Path::new("/sys/module/susfs_guard_lkm").exists() {
        return Ok(true);
    }
    let modules = fs::read_to_string("/proc/modules")?;
    Ok(modules
        .lines()
        .any(|line| line.split_whitespace().next() == Some("susfs_guard_lkm")))
}

fn set_state(status: &mut BootStatus, state: &str, message: impl Into<String>) -> Result<()> {
    status.status.state = state.to_owned();
    status.status.message = message.into();
    save_json(STATUS_PATH, status)
}

pub fn get_status() -> Result<Status> {
    let config = read_json::<Config>(CONFIG_PATH)?.unwrap_or_default();
    let mut status = initial_status(&config);
    if let Some(record) = read_json::<BootStatus>(STATUS_PATH)? {
        let same_boot = record.boot_id == boot_id()?
            && !matches!(record.status.state.as_str(), "pending" | "disabled");
        let retained_failure =
            matches!(record.status.state.as_str(), "failed" | "recovery_disabled")
                && !config.enabled;
        if same_boot || retained_failure {
            status.state = record.status.state;
            status.message = record.status.message;
        }
    }
    if matches!(status.state.as_str(), "disabled" | "pending") {
        match show::probe() {
            Ok(Some(version)) => {
                status.state = "already_active".to_owned();
                status.message = format!(
                    "SUSFS {version} is already active; no additional module will be loaded"
                );
            }
            Err(error) => {
                status.state = "failed".to_owned();
                status.message = format!("{error:#}");
            }
            Ok(None) => {}
        }
    }
    Ok(status)
}

pub fn run(command: LkmCommand) -> Result<()> {
    match command {
        LkmCommand::Status => println!("{}", serde_json::to_string(&get_status()?)?),
        LkmCommand::Enable => {
            let _lock = lock()?;
            let config = Config { enabled: true };
            let status = initial_status(&config);
            ensure!(status.supported && status.available, "{}", status.message);
            ensure_boot_daemon()?;
            save_json(CONFIG_PATH, &config)?;
            save_json(
                STATUS_PATH,
                &BootStatus {
                    boot_id: boot_id()?,
                    status,
                },
            )?;
        }
        LkmCommand::Disable => {
            let _lock = lock()?;
            save_json(CONFIG_PATH, &Config { enabled: false })?;
        }
    }
    Ok(())
}

fn ensure_boot_daemon() -> Result<()> {
    crate::android::ksucalls::ensure_uapi_version_matched()?;
    let current = fs::read("/proc/self/exe")?;
    if fs::read(defs::DAEMON_PATH).is_ok_and(|installed| installed == current) {
        return Ok(());
    }
    crate::android::utils::install(None, None)
        .context("Cannot install the native SUSFS boot daemon")?;
    ensure!(
        fs::read(defs::DAEMON_PATH)? == current,
        "The installed boot daemon does not match this manager"
    );
    Ok(())
}

pub fn on_post_fs_data() -> Result<()> {
    let _lock = lock()?;
    let id = boot_id()?;
    let mut config = read_json::<Config>(CONFIG_PATH)?.unwrap_or_default();
    let mut record = BootStatus {
        boot_id: id.clone(),
        status: initial_status(&config),
    };
    match read_json::<BootStatus>(STATUS_PATH) {
        Ok(Some(previous)) if previous.boot_id == id => return Ok(()),
        Ok(Some(previous)) if previous.status.state == "loading" => {
            config.enabled = false;
            save_json(CONFIG_PATH, &config)?;
            record.status.enabled = false;
            return set_state(
                &mut record,
                "recovery_disabled",
                "The previous SUSFS LKM load did not finish. Autoload was disabled; inspect the boot logs before retrying",
            );
        }
        Ok(Some(previous))
            if !config.enabled
                && matches!(
                    previous.status.state.as_str(),
                    "failed" | "recovery_disabled"
                ) =>
        {
            record.status.state = previous.status.state;
            record.status.message = previous.status.message;
        }
        Err(error) => {
            config.enabled = false;
            save_json(CONFIG_PATH, &config)?;
            record.status.enabled = false;
            return set_state(
                &mut record,
                "recovery_disabled",
                format!("Loader journal could not be read; autoload was disabled: {error:#}"),
            );
        }
        _ => {}
    }
    if !config.enabled || !record.status.supported || !record.status.available {
        return save_json(STATUS_PATH, &record);
    }
    let result = (|| -> Result<()> {
        if module_present()? {
            return set_state(
                &mut record,
                "already_active",
                "A SUSFS LKM is already present; loading was skipped",
            );
        }
        if let Some(version) = show::probe()? {
            return set_state(
                &mut record,
                "already_active",
                format!("SUSFS {version} is already active; loading was skipped"),
            );
        }
        set_state(&mut record, "loading", "Loading the embedded SUSFS LKM")?;
        ksuinit::load_module_checked(
            &assets::get_asset(MODULE_ASSET)?,
            c"expose_proc=1 su_ctx=u:r:ksu:s0 avc_su_ctx=u:r:ksu:s0",
        )?;
        let version = show::probe()?.context("SUSFS did not respond after module loading")?;
        ensure!(
            version == "v2.3.0",
            "Unexpected SUSFS API version: {version}"
        );
        let variant = show::variant()?;
        ensure!(variant == "GKI", "Unexpected SUSFS variant: {variant}");
        let features = show::enabled_features()?;
        for feature in REQUIRED_FEATURES {
            ensure!(
                features.lines().any(|line| line.trim() == *feature),
                "SUSFS did not activate {feature}"
            );
        }
        log::info!(
            "Native SUSFS LKM {} activated:\n{features}",
            record.status.version
        );
        assets::reconcile_susfs_link()?;
        set_state(
            &mut record,
            "loaded",
            format!("SUSFS {version} ({variant}) loaded before native configuration"),
        )
    })();
    if let Err(error) = result {
        config.enabled = false;
        save_json(CONFIG_PATH, &config)?;
        record.status.enabled = false;
        let message =
            format!("{error:#}. Autoload was disabled; a reboot is required before retrying");
        log::warn!("{message}");
        set_state(&mut record, "failed", message)?;
        bail!("{}", record.status.message);
    }
    Ok(())
}
