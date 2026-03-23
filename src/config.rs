use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Config {
    pub connectivity_check_enabled: bool,
    pub connectivity_check_uri: String,
    pub default_wifi_iface: String,
    pub hostname_path: PathBuf,
    pub iwd_state_dir: PathBuf,
    pub iw_bin: String,
    pub iwctl_bin: String,
    pub network_dir: PathBuf,
    pub networkctl_bin: String,
    pub resolv_conf_path: PathBuf,
    pub sync_enabled: bool,
    pub sync_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connectivity_check_enabled: false,
            connectivity_check_uri: String::new(),
            default_wifi_iface: String::from("wlan0"),
            hostname_path: PathBuf::from("/etc/hostname"),
            iwd_state_dir: PathBuf::from("/var/lib/iwd"),
            iw_bin: String::from("iw"),
            iwctl_bin: String::from("iwctl"),
            network_dir: PathBuf::from("/etc/systemd/network"),
            networkctl_bin: String::from("networkctl"),
            resolv_conf_path: PathBuf::from("/etc/resolv.conf"),
            sync_enabled: true,
            sync_interval: Duration::from_secs(5),
        }
    }
}

fn override_slot() -> &'static Mutex<Option<Config>> {
    static SLOT: OnceLock<Mutex<Option<Config>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

pub fn current() -> Config {
    override_slot()
        .lock()
        .expect("config mutex poisoned")
        .clone()
        .unwrap_or_default()
}

pub fn set_override(config: Config) {
    *override_slot().lock().expect("config mutex poisoned") = Some(config);
}

pub fn clear_override() {
    *override_slot().lock().expect("config mutex poisoned") = None;
}
