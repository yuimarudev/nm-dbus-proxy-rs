use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::{
    enums::NM80211Mode,
    network_manager::{
        active_connection::ActiveConnection, settings_connection::ConnectionSettings,
    },
    systemd_networkd::link::{Kind, Type},
};

#[derive(Clone, Debug, Default)]
pub struct Runtime {
    inner: Arc<Mutex<RuntimeState>>,
}

impl Runtime {
    pub fn new(connections: Vec<ConnectionRecord>) -> Self {
        let next_settings_id = connections
            .iter()
            .filter_map(|connection| {
                connection
                    .path
                    .as_str()
                    .rsplit('/')
                    .next()
                    .and_then(|value| value.parse::<usize>().ok())
            })
            .max()
            .unwrap_or(0)
            + 1;

        Self {
            inner: Arc::new(Mutex::new(RuntimeState {
                active_connections: Vec::new(),
                access_points: Vec::new(),
                checkpoints: Vec::new(),
                connectivity_check_enabled: false,
                connections,
                devices: Vec::new(),
                device_managed: HashMap::new(),
                global_dns_configuration: HashMap::new(),
                hostname: String::new(),
                logging_domains: String::from("DEFAULT"),
                logging_level: String::from("INFO"),
                networking_enabled: true,
                next_settings_id,
                next_checkpoint_id: 1,
                registered_agents: Vec::new(),
                signal_level_agents: HashMap::new(),
                sleeping: false,
                version_id: 1,
                wimax_enabled: false,
                wireless_enabled: true,
                wireless_devices: Vec::new(),
                wwan_enabled: true,
            })),
        }
    }

    pub fn active_connection(&self, path: &OwnedObjectPath) -> Option<ActiveConnectionRecord> {
        self.with_state(|state| {
            state
                .active_connections
                .iter()
                .find(|record| &record.path == path)
                .cloned()
        })
    }

    pub fn active_connection_for_interface(&self, interface_name: &str) -> Option<OwnedObjectPath> {
        self.with_state(|state| {
            state
                .active_connections
                .iter()
                .find(|record| {
                    record
                        .value
                        .devices
                        .iter()
                        .any(|path| path.as_str().ends_with(interface_name))
                })
                .map(|record| record.path.clone())
        })
    }

    pub fn active_connection_paths(&self) -> Vec<OwnedObjectPath> {
        self.with_state(|state| {
            state
                .active_connections
                .iter()
                .map(|record| record.path.clone())
                .collect()
        })
    }

    pub fn checkpoint_snapshot(&self, devices: &[OwnedObjectPath]) -> CheckpointSnapshot {
        self.with_state(|state| {
            let interfaces = devices
                .iter()
                .filter_map(|path| path.as_str().rsplit('/').next().map(ToString::to_string))
                .collect::<Vec<_>>();
            let active_connections = state
                .active_connections
                .iter()
                .filter(|record| {
                    record
                        .value
                        .devices
                        .iter()
                        .filter_map(|path| path.as_str().rsplit('/').next())
                        .any(|interface| interfaces.iter().any(|item| item == interface))
                })
                .cloned()
                .collect();
            let connections = state
                .connections
                .iter()
                .filter(|record| {
                    record
                        .interface_name()
                        .map(|interface| interfaces.iter().any(|item| item == &interface))
                        .unwrap_or(false)
                })
                .cloned()
                .collect();
            let device_managed = state
                .device_managed
                .iter()
                .filter(|(interface, _)| interfaces.iter().any(|item| item == *interface))
                .map(|(interface, managed)| (interface.clone(), *managed))
                .collect();

            CheckpointSnapshot {
                active_connections,
                connections,
                device_managed,
                hostname: state.hostname.clone(),
                networking_enabled: state.networking_enabled,
                persisted_files: Vec::new(),
                sleeping: state.sleeping,
            }
        })
    }

    pub fn add_active_connection(&self, record: ActiveConnectionRecord) {
        self.with_state(|state| {
            state
                .active_connections
                .retain(|existing| existing.path != record.path);
            state.active_connections.push(record);
        });
    }

    pub fn add_connection(&self, connection: ConnectionRecord) {
        self.with_state(|state| {
            state.connections.push(connection);
            state.version_id += 1;
        });
    }

    pub fn add_registered_agent(&self, record: RegisteredAgent) {
        self.with_state(|state| {
            state
                .registered_agents
                .retain(|existing| existing.sender != record.sender);
            state.registered_agents.push(record);
        });
    }

    pub fn add_checkpoint(&self, record: CheckpointRecord) {
        self.with_state(|state| state.checkpoints.push(record));
    }

    pub fn access_point(&self, path: &OwnedObjectPath) -> Option<AccessPointRecord> {
        self.with_state(|state| {
            state
                .access_points
                .iter()
                .find(|record| &record.path == path)
                .cloned()
        })
    }

    pub fn access_point_paths(&self) -> Vec<OwnedObjectPath> {
        self.with_state(|state| {
            state
                .access_points
                .iter()
                .map(|record| record.path.clone())
                .collect()
        })
    }

    pub fn connection(&self, path: &OwnedObjectPath) -> Option<ConnectionRecord> {
        self.with_state(|state| {
            state
                .connections
                .iter()
                .find(|record| &record.path == path)
                .cloned()
        })
    }

    pub fn connection_by_uuid(&self, uuid: &str) -> Option<ConnectionRecord> {
        self.with_state(|state| {
            state
                .connections
                .iter()
                .find(|record| record.uuid == uuid)
                .cloned()
        })
    }

    pub fn connections(&self) -> Vec<ConnectionRecord> {
        self.with_state(|state| state.connections.clone())
    }

    pub fn device(&self, path: &OwnedObjectPath) -> Option<DeviceRecord> {
        self.with_state(|state| {
            state
                .devices
                .iter()
                .find(|record| &record.path == path)
                .cloned()
        })
    }

    pub fn device_by_interface(&self, interface_name: &str) -> Option<DeviceRecord> {
        self.with_state(|state| {
            state
                .devices
                .iter()
                .find(|record| record.interface_name == interface_name)
                .cloned()
        })
    }

    pub fn device_paths(&self) -> Vec<OwnedObjectPath> {
        self.with_state(|state| {
            state
                .devices
                .iter()
                .map(|record| record.path.clone())
                .collect()
        })
    }

    pub fn connections_for_interface(&self, interface_name: &str) -> Vec<OwnedObjectPath> {
        self.with_state(|state| {
            state
                .connections
                .iter()
                .filter(|record| record.interface_name().as_deref() == Some(interface_name))
                .map(|record| record.path.clone())
                .collect()
        })
    }

    pub fn checkpoint(&self, path: &OwnedObjectPath) -> Option<CheckpointRecord> {
        self.with_state(|state| {
            state
                .checkpoints
                .iter()
                .find(|record| &record.path == path)
                .cloned()
        })
    }

    pub fn checkpoint_paths(&self) -> Vec<OwnedObjectPath> {
        self.with_state(|state| {
            state
                .checkpoints
                .iter()
                .map(|record| record.path.clone())
                .collect()
        })
    }

    pub fn device_managed(&self, interface_name: &str) -> Option<bool> {
        self.with_state(|state| state.device_managed.get(interface_name).copied())
    }

    pub fn hostname(&self) -> String {
        self.with_state(|state| state.hostname.clone())
    }

    pub fn connectivity_check_enabled(&self) -> bool {
        self.with_state(|state| state.connectivity_check_enabled)
    }

    pub fn global_dns_configuration(&self) -> HashMap<String, OwnedValue> {
        self.with_state(|state| state.global_dns_configuration.clone())
    }

    pub fn logging(&self) -> (String, String) {
        self.with_state(|state| (state.logging_level.clone(), state.logging_domains.clone()))
    }

    pub fn networking_enabled(&self) -> bool {
        self.with_state(|state| state.networking_enabled)
    }

    pub fn registered_agents(&self) -> Vec<RegisteredAgent> {
        self.with_state(|state| state.registered_agents.clone())
    }

    pub fn add_signal_level_agent(&self, interface_name: &str, agent_path: OwnedObjectPath) {
        self.with_state(|state| {
            state
                .signal_level_agents
                .entry(interface_name.to_string())
                .or_default()
                .push(agent_path);
        });
    }

    pub fn remove_signal_level_agent(&self, interface_name: &str, agent_path: &OwnedObjectPath) {
        self.with_state(|state| {
            if let Some(paths) = state.signal_level_agents.get_mut(interface_name) {
                paths.retain(|path| path != agent_path);
            }
        });
    }

    pub fn remove_wireless_device(&self, interface_name: &str) -> Option<WirelessDeviceRecord> {
        self.with_state(|state| {
            let position = state
                .wireless_devices
                .iter()
                .position(|record| record.interface_name == interface_name)?;
            state.signal_level_agents.remove(interface_name);
            Some(state.wireless_devices.remove(position))
        })
    }

    pub fn sleeping(&self) -> bool {
        self.with_state(|state| state.sleeping)
    }

    pub fn interface_for_access_point(&self, path: &OwnedObjectPath) -> Option<String> {
        self.with_state(|state| {
            state
                .wireless_devices
                .iter()
                .find(|record| record.access_points.iter().any(|item| item == path))
                .map(|record| record.interface_name.clone())
        })
    }

    pub fn wimax_enabled(&self) -> bool {
        self.with_state(|state| state.wimax_enabled)
    }

    pub fn wireless_device(&self, interface_name: &str) -> Option<WirelessDeviceRecord> {
        self.with_state(|state| {
            state
                .wireless_devices
                .iter()
                .find(|record| record.interface_name == interface_name)
                .cloned()
        })
    }

    pub fn upsert_wireless_device(&self, record: WirelessDeviceRecord) {
        self.with_state(|state| {
            if let Some(existing) = state
                .wireless_devices
                .iter_mut()
                .find(|existing| existing.interface_name == record.interface_name)
            {
                *existing = record;
            } else {
                state.wireless_devices.push(record);
            }
        });
    }

    pub fn update_wireless_device<F, T>(&self, interface_name: &str, update: F) -> Option<T>
    where
        F: FnOnce(&mut WirelessDeviceRecord) -> T,
    {
        self.with_state(|state| {
            let record = state
                .wireless_devices
                .iter_mut()
                .find(|record| record.interface_name == interface_name)?;
            Some(update(record))
        })
    }

    pub fn wireless_enabled(&self) -> bool {
        self.with_state(|state| state.wireless_enabled)
    }

    pub fn wwan_enabled(&self) -> bool {
        self.with_state(|state| state.wwan_enabled)
    }

    pub fn next_connection_path(&self) -> OwnedObjectPath {
        self.with_state(|state| {
            let path = OwnedObjectPath::try_from(
                format!(
                    "/org/freedesktop/NetworkManager/Settings/{}",
                    state.next_settings_id
                )
                .as_str(),
            )
            .expect("generated settings path should be valid");
            state.next_settings_id += 1;
            path
        })
    }

    pub fn next_checkpoint_path(&self) -> OwnedObjectPath {
        self.with_state(|state| {
            let path = OwnedObjectPath::try_from(
                format!(
                    "/org/freedesktop/NetworkManager/Checkpoint/{}",
                    state.next_checkpoint_id
                )
                .as_str(),
            )
            .expect("generated checkpoint path should be valid");
            state.next_checkpoint_id += 1;
            path
        })
    }

    pub fn remove_active_connection(
        &self,
        path: &OwnedObjectPath,
    ) -> Option<ActiveConnectionRecord> {
        self.with_state(|state| {
            let position = state
                .active_connections
                .iter()
                .position(|record| &record.path == path)?;
            Some(state.active_connections.remove(position))
        })
    }

    pub fn remove_access_point(&self, path: &OwnedObjectPath) -> Option<AccessPointRecord> {
        self.with_state(|state| {
            let position = state
                .access_points
                .iter()
                .position(|record| &record.path == path)?;
            Some(state.access_points.remove(position))
        })
    }

    pub fn remove_connection(&self, path: &OwnedObjectPath) -> Option<ConnectionRecord> {
        self.with_state(|state| {
            let position = state
                .connections
                .iter()
                .position(|record| &record.path == path)?;
            state.version_id += 1;
            Some(state.connections.remove(position))
        })
    }

    pub fn remove_device(&self, path: &OwnedObjectPath) -> Option<DeviceRecord> {
        self.with_state(|state| {
            let position = state
                .devices
                .iter()
                .position(|record| &record.path == path)?;
            Some(state.devices.remove(position))
        })
    }

    pub fn remove_checkpoint(&self, path: &OwnedObjectPath) -> Option<CheckpointRecord> {
        self.with_state(|state| {
            let position = state
                .checkpoints
                .iter()
                .position(|record| &record.path == path)?;
            Some(state.checkpoints.remove(position))
        })
    }

    pub fn remove_registered_agent(&self, sender: &str) -> Option<RegisteredAgent> {
        self.with_state(|state| {
            let position = state
                .registered_agents
                .iter()
                .position(|record| record.sender == sender)?;
            Some(state.registered_agents.remove(position))
        })
    }

    pub fn set_hostname(&self, hostname: String) {
        self.with_state(|state| {
            state.hostname = hostname;
            state.version_id += 1;
        });
    }

    pub fn set_device_managed(&self, interface_name: &str, managed: bool) {
        self.with_state(|state| {
            state
                .device_managed
                .insert(interface_name.to_string(), managed);
        });
    }

    pub fn set_networking_enabled(&self, enabled: bool) {
        self.with_state(|state| {
            state.networking_enabled = enabled;
        });
    }

    pub fn set_connectivity_check_enabled(&self, enabled: bool) {
        self.with_state(|state| {
            state.connectivity_check_enabled = enabled;
        });
    }

    pub fn set_global_dns_configuration(&self, configuration: HashMap<String, OwnedValue>) {
        self.with_state(|state| {
            state.global_dns_configuration = configuration;
        });
    }

    pub fn set_logging(&self, level: String, domains: String) {
        self.with_state(|state| {
            state.logging_level = level;
            state.logging_domains = domains;
        });
    }

    pub fn set_sleeping(&self, sleeping: bool) {
        self.with_state(|state| {
            state.sleeping = sleeping;
        });
    }

    pub fn set_wimax_enabled(&self, enabled: bool) {
        self.with_state(|state| {
            state.wimax_enabled = enabled;
        });
    }

    pub fn set_wireless_enabled(&self, enabled: bool) {
        self.with_state(|state| {
            state.wireless_enabled = enabled;
        });
    }

    pub fn set_wwan_enabled(&self, enabled: bool) {
        self.with_state(|state| {
            state.wwan_enabled = enabled;
        });
    }

    pub fn update_connection<F, T>(&self, path: &OwnedObjectPath, update: F) -> Option<T>
    where
        F: FnOnce(&mut ConnectionRecord) -> T,
    {
        self.with_state(|state| {
            let record = state
                .connections
                .iter_mut()
                .find(|record| &record.path == path)?;
            let result = update(record);
            state.version_id += 1;
            Some(result)
        })
    }

    pub fn upsert_access_point(&self, record: AccessPointRecord) {
        self.with_state(|state| {
            if let Some(existing) = state
                .access_points
                .iter_mut()
                .find(|existing| existing.path == record.path)
            {
                *existing = record;
            } else {
                state.access_points.push(record);
            }
        });
    }

    pub fn upsert_device(&self, record: DeviceRecord) {
        self.with_state(|state| {
            if let Some(existing) = state
                .devices
                .iter_mut()
                .find(|existing| existing.path == record.path)
            {
                *existing = record;
            } else {
                state.devices.push(record);
            }
        });
    }

    pub fn restore_checkpoint_snapshot(&self, snapshot: &CheckpointSnapshot) {
        self.with_state(|state| {
            state.connections = snapshot.connections.clone();
            state.active_connections = snapshot.active_connections.clone();
            state.device_managed = snapshot.device_managed.clone();
            state.hostname = snapshot.hostname.clone();
            state.networking_enabled = snapshot.networking_enabled;
            state.sleeping = snapshot.sleeping;
            state.version_id += 1;
        });
    }

    pub fn version_id(&self) -> u64 {
        self.with_state(|state| state.version_id)
    }

    pub fn update_checkpoint<F, T>(&self, path: &OwnedObjectPath, update: F) -> Option<T>
    where
        F: FnOnce(&mut CheckpointRecord) -> T,
    {
        self.with_state(|state| {
            let record = state
                .checkpoints
                .iter_mut()
                .find(|record| &record.path == path)?;
            Some(update(record))
        })
    }

    pub fn checkpoint_rollback_timeout(
        &self,
        path: &OwnedObjectPath,
        now_millis: i64,
    ) -> Option<u32> {
        self.with_state(|state| {
            let record = state
                .checkpoints
                .iter()
                .find(|record| &record.path == path)?;
            Some(match record.rollback_deadline_millis {
                Some(deadline) if deadline > now_millis => ((deadline - now_millis + 999) / 1000)
                    .try_into()
                    .unwrap_or(u32::MAX),
                Some(_) => 0,
                None => record.rollback_timeout,
            })
        })
    }

    fn with_state<F, T>(&self, callback: F) -> T
    where
        F: FnOnce(&mut RuntimeState) -> T,
    {
        let mut state = self.inner.lock().expect("runtime mutex poisoned");
        callback(&mut state)
    }
}

#[derive(Clone, Debug)]
pub struct ActiveConnectionRecord {
    pub path: OwnedObjectPath,
    pub value: ActiveConnection,
}

#[derive(Clone, Debug)]
pub struct RegisteredAgent {
    pub capabilities: u32,
    pub identifier: String,
    pub sender: String,
}

#[derive(Clone, Debug)]
pub struct CheckpointRecord {
    pub created: i64,
    pub devices: Vec<OwnedObjectPath>,
    pub path: OwnedObjectPath,
    pub rollback_timeout: u32,
    pub rollback_deadline_millis: Option<i64>,
    pub snapshot: CheckpointSnapshot,
}

#[derive(Clone, Debug, Default)]
pub struct CheckpointSnapshot {
    pub active_connections: Vec<ActiveConnectionRecord>,
    pub connections: Vec<ConnectionRecord>,
    pub device_managed: HashMap<String, bool>,
    pub hostname: String,
    pub networking_enabled: bool,
    pub persisted_files: Vec<(String, Vec<u8>)>,
    pub sleeping: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AccessPointRecord {
    pub bandwidth: u32,
    pub flags: u32,
    pub frequency: u32,
    pub hw_address: String,
    pub last_seen: i32,
    pub max_bitrate: u32,
    pub mode: NM80211Mode,
    pub path: OwnedObjectPath,
    pub rsn_flags: u32,
    pub ssid: String,
    pub strength: u8,
    pub wpa_flags: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WirelessDeviceRecord {
    pub access_points: Vec<OwnedObjectPath>,
    pub active_access_point: OwnedObjectPath,
    pub bitrate: u32,
    pub interface_name: String,
    pub last_scan: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceRecord {
    pub dhcp4_config: OwnedObjectPath,
    pub dhcp6_config: OwnedObjectPath,
    pub interface_name: String,
    pub is_ppp: bool,
    pub ip4_config: OwnedObjectPath,
    pub ip6_config: OwnedObjectPath,
    pub kind: Kind,
    pub path: OwnedObjectPath,
    pub p2p_peers: Vec<OwnedObjectPath>,
    pub type_: Type,
    pub wifi_p2p: bool,
}

#[derive(Clone, Debug)]
pub struct ConnectionRecord {
    pub connection_type: String,
    pub filename: String,
    pub flags: u32,
    pub origin: ConnectionOrigin,
    pub path: OwnedObjectPath,
    pub settings: ConnectionSettings,
    pub unsaved: bool,
    pub uuid: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionOrigin {
    BackendWired,
    BackendWifi,
    #[default]
    User,
}

impl ConnectionRecord {
    pub fn autoconnect(&self) -> bool {
        self.setting_bool("connection", "autoconnect")
    }

    pub fn id(&self) -> String {
        self.setting("connection", "id")
            .unwrap_or_else(|| self.path.as_str().to_string())
    }

    pub fn interface_name(&self) -> Option<String> {
        self.setting("connection", "interface-name")
    }

    pub fn is_hidden(&self) -> bool {
        self.setting_bool("802-11-wireless", "hidden")
    }

    pub fn ssid(&self) -> Option<String> {
        self.settings
            .get("802-11-wireless")
            .and_then(|wireless| wireless.get("ssid"))
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| Vec::<u8>::try_from(value).ok())
            .and_then(|ssid| String::from_utf8(ssid).ok())
    }

    pub fn wifi_passphrase(&self) -> Option<String> {
        self.setting("802-11-wireless-security", "psk")
    }

    pub fn with_settings(&mut self, settings: ConnectionSettings) {
        self.settings = settings;
        if let Some(uuid) = self.setting("connection", "uuid") {
            self.uuid = uuid;
        }
        if let Some(connection_type) = self.setting("connection", "type") {
            self.connection_type = connection_type;
        }
    }

    fn setting(&self, group: &str, key: &str) -> Option<String> {
        self.settings
            .get(group)
            .and_then(|settings| settings.get(key))
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| String::try_from(value).ok())
    }

    fn setting_bool(&self, group: &str, key: &str) -> bool {
        self.settings
            .get(group)
            .and_then(|settings| settings.get(key))
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or(false)
    }
}

#[derive(Debug, Default)]
struct RuntimeState {
    active_connections: Vec<ActiveConnectionRecord>,
    access_points: Vec<AccessPointRecord>,
    checkpoints: Vec<CheckpointRecord>,
    connectivity_check_enabled: bool,
    connections: Vec<ConnectionRecord>,
    devices: Vec<DeviceRecord>,
    device_managed: HashMap<String, bool>,
    global_dns_configuration: HashMap<String, OwnedValue>,
    hostname: String,
    logging_domains: String,
    logging_level: String,
    networking_enabled: bool,
    next_checkpoint_id: usize,
    next_settings_id: usize,
    registered_agents: Vec<RegisteredAgent>,
    signal_level_agents: HashMap<String, Vec<OwnedObjectPath>>,
    sleeping: bool,
    version_id: u64,
    wimax_enabled: bool,
    wireless_enabled: bool,
    wireless_devices: Vec<WirelessDeviceRecord>,
    wwan_enabled: bool,
}
