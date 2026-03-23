use std::{collections::HashMap, fs, path::PathBuf};

use zbus::{
    interface,
    zvariant::{OwnedValue, Value},
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DnsManager {
    pub configuration: Vec<HashMap<String, OwnedValue>>,
    pub mode: String,
    pub rc_manager: String,
}

#[interface(name = "org.freedesktop.NetworkManager.DnsManager")]
impl DnsManager {
    #[zbus(property)]
    fn configuration(&self) -> Vec<HashMap<String, OwnedValue>> {
        current_configuration().unwrap_or_else(|| self.configuration.clone())
    }

    #[zbus(property)]
    fn mode(&self) -> String {
        current_mode().unwrap_or_else(|| self.mode.clone())
    }

    #[zbus(property)]
    fn rc_manager(&self) -> String {
        current_rc_manager().unwrap_or_else(|| self.rc_manager.clone())
    }
}

pub(crate) fn current_global_configuration() -> HashMap<String, OwnedValue> {
    let parsed = parse_resolv_conf();
    let mut out = HashMap::new();

    if !parsed.nameservers.is_empty() {
        out.insert(
            String::from("nameservers"),
            OwnedValue::try_from(Value::from(parsed.nameservers))
                .expect("nameservers should fit in OwnedValue"),
        );
    }
    if !parsed.domains.is_empty() {
        out.insert(
            String::from("domains"),
            OwnedValue::try_from(Value::from(parsed.domains))
                .expect("domains should fit in OwnedValue"),
        );
    }

    out
}

pub(crate) fn current_domains() -> Vec<String> {
    parse_resolv_conf().domains
}

pub(crate) fn current_nameservers() -> Vec<String> {
    parse_resolv_conf().nameservers
}

fn current_configuration() -> Option<Vec<HashMap<String, OwnedValue>>> {
    let configuration = current_global_configuration();
    if configuration.is_empty() {
        None
    } else {
        Some(vec![configuration])
    }
}

fn current_mode() -> Option<String> {
    if resolv_conf_path().exists() {
        Some(String::from("default"))
    } else {
        None
    }
}

fn current_rc_manager() -> Option<String> {
    if resolv_conf_path().exists() {
        Some(String::from("file"))
    } else {
        None
    }
}

#[derive(Default)]
struct ResolvConf {
    domains: Vec<String>,
    nameservers: Vec<String>,
}

fn parse_resolv_conf() -> ResolvConf {
    let Ok(contents) = fs::read_to_string(resolv_conf_path()) else {
        return ResolvConf::default();
    };

    let mut out = ResolvConf::default();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(keyword) = fields.next() else {
            continue;
        };

        match keyword {
            "nameserver" => {
                if let Some(value) = fields.next() {
                    out.nameservers.push(value.to_string());
                }
            }
            "search" | "domain" => {
                out.domains.extend(fields.map(ToString::to_string));
            }
            _ => {}
        }
    }

    out
}

fn resolv_conf_path() -> PathBuf {
    crate::config::current().resolv_conf_path
}
