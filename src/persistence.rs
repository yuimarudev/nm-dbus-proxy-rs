use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
};

use tokio::fs;

use crate::runtime::ConnectionRecord;

pub async fn persist_connection(record: &mut ConnectionRecord) -> Result<(), String> {
    let filename = filename_for_connection(record)?;
    let contents = match record.connection_type.as_str() {
        "802-11-wireless" => wifi_profile(record)?,
        "802-3-ethernet" => wired_profile(record)?,
        kind if netdev_kind_for_connection_type(kind).is_some() => netdev_profile(record)?,
        _ => wired_profile(record)?,
    };

    if let Some(parent) = filename.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    fs::write(&filename, contents)
        .await
        .map_err(|error| error.to_string())?;
    record.filename = filename.display().to_string();

    Ok(())
}

pub async fn delete_persisted_connection(record: &ConnectionRecord) -> Result<(), String> {
    let path = PathBuf::from(&record.filename);
    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(path)
        .await
        .map_err(|error| error.to_string())
}

pub async fn discover_connection_files() -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_connection_files(iwd_state_dir(), &mut files).await?;
    collect_connection_files(networkd_config_dir(), &mut files).await?;
    Ok(files)
}

pub async fn snapshot_connection_files() -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut snapshot = Vec::new();
    for path in discover_connection_files().await? {
        let contents = fs::read(&path).await.map_err(|error| error.to_string())?;
        snapshot.push((path.display().to_string(), contents));
    }
    Ok(snapshot)
}

pub async fn restore_connection_files(snapshot: &[(String, Vec<u8>)]) -> Result<(), String> {
    let current = discover_connection_files().await?;
    for path in current {
        if snapshot
            .iter()
            .any(|(saved_path, _)| saved_path == &path.display().to_string())
        {
            continue;
        }
        let _ = fs::remove_file(path).await;
    }

    for (path, contents) in snapshot {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|error| error.to_string())?;
        }
        fs::write(path, contents)
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

pub async fn load_connection_from_path(path: &Path) -> Result<ConnectionRecord, String> {
    let filename = path.display().to_string();
    let contents = fs::read_to_string(path)
        .await
        .map_err(|error| error.to_string())?;

    if path.extension().and_then(|value| value.to_str()) == Some("network") {
        let interface_name =
            parse_network_value(&contents, "Match", "Name").unwrap_or_else(|| String::from("*"));
        return Ok(ConnectionRecord {
            connection_type: String::from("802-3-ethernet"),
            filename,
            flags: 0,
            origin: Default::default(),
            path: Default::default(),
            settings: HashMap::from([
                (
                    String::from("connection"),
                    HashMap::from([
                        (String::from("id"), owned(interface_name.clone())),
                        (String::from("interface-name"), owned(interface_name)),
                        (String::from("type"), owned(String::from("802-3-ethernet"))),
                        (
                            String::from("uuid"),
                            owned(format!("imported-{}", path.display())),
                        ),
                        (String::from("autoconnect"), owned(true)),
                    ]),
                ),
                (String::from("802-3-ethernet"), HashMap::new()),
            ]),
            unsaved: false,
            uuid: format!("imported-{}", path.display()),
        });
    }

    let encoded_ssid = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid Wi-Fi filename '{}'", path.display()))?;
    let ssid = if let Some(encoded) = encoded_ssid.strip_prefix('=') {
        let mut bytes = Vec::with_capacity(encoded.len() / 2);
        for chunk in encoded.as_bytes().chunks(2) {
            let value = std::str::from_utf8(chunk)
                .ok()
                .and_then(|value| u8::from_str_radix(value, 16).ok())
                .ok_or_else(|| format!("invalid encoded SSID '{encoded_ssid}'"))?;
            bytes.push(value);
        }
        String::from_utf8(bytes).map_err(|error| error.to_string())?
    } else {
        encoded_ssid.to_string()
    };
    let passphrase = parse_network_value(&contents, "Security", "Passphrase");
    let uuid = format!("imported-{}", path.display());
    let mut settings = HashMap::from([
        (
            String::from("connection"),
            HashMap::from([
                (String::from("id"), owned(ssid.clone())),
                (
                    String::from("interface-name"),
                    owned(
                        env::var("NM_DBUS_PROXY_DEFAULT_WIFI_IFACE")
                            .unwrap_or_else(|_| String::from("wlan0")),
                    ),
                ),
                (String::from("type"), owned(String::from("802-11-wireless"))),
                (String::from("uuid"), owned(uuid.clone())),
                (String::from("autoconnect"), owned(true)),
            ]),
        ),
        (
            String::from("802-11-wireless"),
            HashMap::from([(String::from("ssid"), owned(ssid.into_bytes()))]),
        ),
    ]);
    if let Some(passphrase) = passphrase {
        settings.insert(
            String::from("802-11-wireless-security"),
            HashMap::from([
                (String::from("key-mgmt"), owned(String::from("wpa-psk"))),
                (String::from("psk"), owned(passphrase)),
            ]),
        );
    }

    Ok(ConnectionRecord {
        connection_type: String::from("802-11-wireless"),
        filename,
        flags: 0,
        origin: Default::default(),
        path: Default::default(),
        settings,
        unsaved: false,
        uuid,
    })
}

fn filename_for_connection(record: &ConnectionRecord) -> Result<PathBuf, String> {
    match record.connection_type.as_str() {
        "802-11-wireless" => {
            let ssid = record
                .ssid()
                .ok_or_else(|| String::from("Wi-Fi connection is missing SSID"))?;
            let extension = if record.wifi_passphrase().is_some() {
                "psk"
            } else {
                "open"
            };
            Ok(iwd_state_dir().join(format!("{}.{}", encode_iwd_name(&ssid), extension)))
        }
        "802-3-ethernet" => {
            Ok(networkd_config_dir().join(format!("90-nm-dbus-proxy-{}.network", record.uuid)))
        }
        kind if netdev_kind_for_connection_type(kind).is_some() => {
            Ok(networkd_config_dir().join(format!("90-nm-dbus-proxy-{}.netdev", record.uuid)))
        }
        _ => Ok(networkd_config_dir().join(format!("90-nm-dbus-proxy-{}.network", record.uuid))),
    }
}

fn wifi_profile(record: &ConnectionRecord) -> Result<String, String> {
    let mut out = String::from("[Settings]\n");
    out.push_str(&format!(
        "AutoConnect={}\n",
        if record.autoconnect() {
            "true"
        } else {
            "false"
        }
    ));
    if record.is_hidden() {
        out.push_str("Hidden=true\n");
    }

    if let Some(passphrase) = record.wifi_passphrase() {
        out.push_str("\n[Security]\n");
        out.push_str(&format!("Passphrase={passphrase}\n"));
    }

    Ok(out)
}

fn wired_profile(record: &ConnectionRecord) -> Result<String, String> {
    let mut out = String::from("[Match]\n");
    if let Some(interface_name) = record.interface_name() {
        out.push_str(&format!("Name={interface_name}\n"));
    } else {
        out.push_str("Name=*\n");
    }

    out.push_str("\n[Network]\nDHCP=yes\n");
    Ok(out)
}

fn netdev_profile(record: &ConnectionRecord) -> Result<String, String> {
    let interface_name = record
        .interface_name()
        .ok_or_else(|| String::from("networkd connection is missing interface-name"))?;
    let kind = netdev_kind_for_connection_type(&record.connection_type).unwrap_or("dummy");
    Ok(format!("[NetDev]\nName={interface_name}\nKind={kind}\n"))
}

fn encode_iwd_name(ssid: &str) -> String {
    if ssid
        .chars()
        .all(|char| char.is_ascii_alphanumeric() || matches!(char, ' ' | '_' | '-'))
    {
        ssid.to_string()
    } else {
        format!(
            "={}",
            ssid.as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }
}

fn parse_network_value(contents: &str, section: &str, key: &str) -> Option<String> {
    let mut current_section = String::new();
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line.trim_matches(['[', ']']).to_string();
            continue;
        }
        if current_section != section {
            continue;
        }
        let (current_key, value) = line.split_once('=')?;
        if current_key.trim() == key {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn owned<T>(value: T) -> zbus::zvariant::OwnedValue
where
    T: Into<zbus::zvariant::Value<'static>>,
{
    zbus::zvariant::OwnedValue::try_from(value.into()).expect("value should fit")
}

async fn collect_connection_files(dir: PathBuf, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let Ok(mut entries) = fs::read_dir(&dir).await else {
        return Ok(());
    };

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| error.to_string())?
    {
        let path = entry.path();
        if path.is_file() {
            out.push(path);
        }
    }

    Ok(())
}

fn iwd_state_dir() -> PathBuf {
    if let Ok(path) = env::var("NM_DBUS_PROXY_IWD_STATE_DIR") {
        PathBuf::from(path)
    } else {
        crate::config::current().iwd_state_dir
    }
}

fn networkd_config_dir() -> PathBuf {
    if let Ok(path) = env::var("NM_DBUS_PROXY_NETWORK_DIR") {
        PathBuf::from(path)
    } else {
        crate::config::current().network_dir
    }
}

fn netdev_kind_for_connection_type(connection_type: &str) -> Option<&'static str> {
    match connection_type {
        "bond" => Some("bond"),
        "bridge" => Some("bridge"),
        "6lowpan" => Some("lowpan"),
        "dummy" => Some("dummy"),
        "hsr" => Some("hsr"),
        "ipvlan" => Some("ipvlan"),
        "macvlan" => Some("macvlan"),
        "team" => Some("team"),
        "tun" => Some("tun"),
        "veth" => Some("veth"),
        "vlan" => Some("vlan"),
        "vrf" => Some("vrf"),
        "vxlan" => Some("vxlan"),
        "wireguard" => Some("wireguard"),
        _ => None,
    }
}
