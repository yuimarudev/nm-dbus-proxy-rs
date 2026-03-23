// Modified by yuimarudev on 2026-03-23.
// This file contains changes from the original upstream work.
use std::{collections::HashMap, fs, time::Duration};

use anyhow::Result;
use futures_util::{StreamExt, stream::SelectAll};
use iwd::{KnownNetwork, State as IwdState};
use network_manager::{
    NetworkManager,
    access_point::AccessPoint,
    active_connection::ActiveConnection,
    agent_manager::AgentManager,
    dns_manager::DnsManager,
    vpn_connection::VpnConnection,
    vpn_plugin::VpnPlugin,
    wifi_p2p_peer::WifiP2PPeer,
    device::{
        Device, loopback::DeviceLoopback,
        specialized::{
            device_aux_interface_names, maybe_ppp_interface_names, maybe_wifi_p2p_interface_name,
            register_device_aux_interfaces, unregister_device_aux_interfaces,
        },
        wired::DeviceWired, wireless::DeviceWireless,
    },
    settings::Settings,
    settings_connection::{ConnectionSettings, SettingsConnection},
};
use systemd_networkd::{
    Manager,
    link::{Address, Kind, Link, Type},
};
use tokio::time::sleep;
use uuid::Uuid;
use zbus::{
    Address as BusAddress, Connection, MatchRule, MessageStream,
    conn::Builder,
    fdo::{ObjectManager, Properties},
    interface,
    message::Type as MessageType,
    names::InterfaceName,
    object_server::SignalEmitter,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

mod config;
mod enums;
pub mod iwd;
mod network_manager;
mod persistence;
mod runtime;
pub mod systemd_networkd;

pub use config::{
    Config, clear_override as clear_config_override, set_override as set_config_override,
};
pub use runtime::Runtime;

use enums::{
    NM80211ApFlags, NM80211ApSecurityFlags, NM80211Mode, NMActivationStateFlags,
    NMActiveConnectionState, NMConnectivityState, NMDeviceCapabilities, NMDeviceInterfaceFlags,
    NMDeviceState, NMDeviceStateReason, NMDeviceType, NMDeviceWifiCapabilities, NMMetered,
    NMRadioFlags,
};
use runtime::{
    AccessPointRecord, ActiveConnectionRecord, ConnectionRecord, DeviceRecord, WirelessDeviceRecord,
};

const NM_ROOT_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const NM_SETTINGS_CONNECTION_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const NM_ACTIVE_CONNECTIONS_PATH: &str = "/org/freedesktop/NetworkManager/ActiveConnections";
const NM_AGENT_MANAGER_PATH: &str = "/org/freedesktop/NetworkManager/AgentManager";
const NM_DNS_MANAGER_PATH: &str = "/org/freedesktop/NetworkManager/DnsManager";
const NM_VPN_PLUGIN_PATH: &str = "/org/freedesktop/NetworkManager/VPN/Plugin";
const NM_DEVICES_PATH: &str = "/org/freedesktop/NetworkManager/Devices";
const NM_ACCESS_POINT_PATH: &str = "/org/freedesktop/NetworkManager/AccessPoint";
const NM_IP4_CONFIG_PATH: &str = "/org/freedesktop/NetworkManager/IP4Config";
const NM_IP6_CONFIG_PATH: &str = "/org/freedesktop/NetworkManager/IP6Config";
const NM_DHCP4_CONFIG_PATH: &str = "/org/freedesktop/NetworkManager/DHCP4Config";
const NM_DHCP6_CONFIG_PATH: &str = "/org/freedesktop/NetworkManager/DHCP6Config";
const NM_WIFI_P2P_PEER_PATH: &str = "/org/freedesktop/NetworkManager/WifiP2PPeer";

fn object_path_segment(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        let ch = char::from(byte);
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
            out.push_str(&format!("{byte:02x}"));
        }
    }
    if out.is_empty() {
        String::from("_")
    } else {
        out
    }
}

pub(crate) fn device_object_path(interface_name: &str) -> OwnedObjectPath {
    owned_path(format!("{NM_DEVICES_PATH}/{}", object_path_segment(interface_name)).as_str())
}

pub(crate) fn active_connection_object_path(interface_name: &str) -> OwnedObjectPath {
    owned_path(
        format!(
            "{NM_ACTIVE_CONNECTIONS_PATH}/{}",
            object_path_segment(interface_name)
        )
        .as_str(),
    )
}

fn config_object_path(prefix: &str, interface_name: &str) -> OwnedObjectPath {
    owned_path(format!("{prefix}/{}", object_path_segment(interface_name)).as_str())
}

fn wifi_p2p_peer_path(interface_name: &str, network_path: &OwnedObjectPath) -> OwnedObjectPath {
    owned_path(
        format!(
            "{NM_WIFI_P2P_PEER_PATH}/{}_{}",
            object_path_segment(interface_name),
            deterministic_uuid("wifi-p2p-peer", network_path.as_str()).replace('-', "_"),
        )
        .as_str(),
    )
}

pub async fn start_service(
    address: Option<BusAddress>,
    manager: Manager,
    wireless: IwdState,
) -> Result<Connection> {
    let (conn, _) = start_service_with_runtime(address, manager, wireless).await?;
    Ok(conn)
}

pub async fn start_service_with_runtime(
    address: Option<BusAddress>,
    manager: Manager,
    wireless: IwdState,
) -> Result<(Connection, Runtime)> {
    let service_bus = if let Some(some) = address {
        Builder::address(some)?
    } else {
        Builder::system()?
    };
    let conn = service_bus.build().await?;
    let server = conn.object_server();

    let hostname = current_hostname();
    let wireless_interface_name = manager
        .links
        .iter()
        .find(|link| link.description.r#type == Type::Wlan)
        .map(|link| link.description.name.clone())
        .unwrap_or_default();

    let mut connection_records = vec![];
    let mut settings_paths = vec![];
    let mut uuid_to_settings_path = HashMap::new();
    let mut known_network_to_settings_path = HashMap::new();
    let mut wired_settings_by_interface = HashMap::new();
    let mut wifi_settings_paths = vec![];
    let mut settings_index = 0usize;

    for known_network in &wireless.known_networks {
        let path =
            owned_path(format!("{NM_SETTINGS_CONNECTION_PATH}/{}", settings_index + 1).as_str());
        let uuid = deterministic_uuid("wifi", &known_network.path.to_string());
        let settings = build_wifi_settings(&wireless_interface_name, known_network, &uuid);

        connection_records.push(ConnectionRecord {
            connection_type: String::from("802-11-wireless"),
            filename: synthetic_iwd_filename(known_network),
            flags: 0,
            origin: runtime::ConnectionOrigin::BackendWifi,
            path: path.clone(),
            settings,
            unsaved: false,
            uuid: uuid.clone(),
        });
        settings_paths.push(path.clone());
        wifi_settings_paths.push(path.clone());
        uuid_to_settings_path.insert(uuid, path.clone());
        known_network_to_settings_path.insert(known_network.path.clone(), path);
        settings_index += 1;
    }

    for link in &manager.links {
        let Some(connection_type) = connection_type_for_link(link) else {
            continue;
        };
        if connection_type == "802-11-wireless" {
            continue;
        }

        let path =
            owned_path(format!("{NM_SETTINGS_CONNECTION_PATH}/{}", settings_index + 1).as_str());
        let uuid = deterministic_uuid(link_uuid_namespace(link), &link.description.name);

        connection_records.push(ConnectionRecord {
            connection_type: String::from(connection_type),
            filename: synthetic_networkd_filename(link, connection_type),
            flags: 0,
            origin: runtime::ConnectionOrigin::BackendWired,
            path: path.clone(),
            settings: build_link_settings(&link.description.name, connection_type, &uuid),
            unsaved: false,
            uuid: uuid.clone(),
        });
        settings_paths.push(path.clone());
        uuid_to_settings_path.insert(uuid, path.clone());
        wired_settings_by_interface.insert(link.description.name.clone(), path);
        settings_index += 1;
    }

    let runtime = Runtime::new(connection_records);
    runtime.set_connectivity_check_enabled(crate::config::current().connectivity_check_enabled);
    runtime.set_global_dns_configuration(
        crate::network_manager::dns_manager::current_global_configuration(),
    );
    runtime.set_wimax_enabled(false);
    runtime.set_wireless_enabled(
        radio_flags(&manager, &wireless) & (NMRadioFlags::WlanAvailable as u32) != 0,
    );
    runtime.set_wwan_enabled(
        radio_flags(&manager, &wireless) & (NMRadioFlags::WwanAvailable as u32) != 0,
    );
    for connection in runtime.connections() {
        server
            .at(
                connection.path.as_str(),
                SettingsConnection {
                    path: connection.path.clone(),
                    runtime: runtime.clone(),
                },
            )
            .await?;
    }

    server
        .at(
            NM_SETTINGS_PATH,
            Settings {
                can_modify: false,
                hostname,
                runtime: runtime.clone(),
                version_id: 1,
            },
        )
        .await?;

    let mut access_point_by_network_path = HashMap::new();
    let mut scan_cache = HashMap::new();
    for station in &wireless.stations {
        for ordered_network in &station.ordered_networks {
            if access_point_by_network_path.contains_key(&ordered_network.path) {
                continue;
            }

            let Some(network) = wireless.network_by_path(&ordered_network.path) else {
                continue;
            };

            let hardware_address = network
                .extended_service_set
                .first()
                .and_then(|path| wireless.basic_service_set_by_path(path))
                .map(|bss| bss.address.clone())
                .unwrap_or_default();
            let interface_name = wireless
                .devices
                .iter()
                .find(|device| device.path == network.device)
                .map(|device| device.name.clone())
                .unwrap_or_default();
            let metadata = scan_cache
                .entry(interface_name.clone())
                .or_insert_with_key(|interface_name| iw_scan_metadata(interface_name))
                .get(&hardware_address.to_ascii_lowercase())
                .cloned()
                .unwrap_or_default();
            let (flags, wpa_flags, rsn_flags) = ap_security_flags(network.kind.as_str());

            let path = access_point_path(&ordered_network.path);
            let access_point = AccessPoint {
                bandwidth: metadata.bandwidth.max(20),
                flags: metadata.flags | flags,
                frequency: metadata.frequency,
                hw_address: hardware_address,
                last_seen: metadata.last_seen,
                max_bitrate: metadata.max_bitrate,
                mode: NM80211Mode::Infra,
                path: path.clone(),
                rsn_flags: metadata.rsn_flags | rsn_flags,
                runtime: runtime.clone(),
                ssid: network.name.clone(),
                strength: signal_to_strength(ordered_network.signal),
                wpa_flags: metadata.wpa_flags | wpa_flags,
            };
            runtime.upsert_access_point(AccessPointRecord {
                bandwidth: access_point.bandwidth,
                flags: access_point.flags,
                frequency: access_point.frequency,
                hw_address: access_point.hw_address.clone(),
                last_seen: access_point.last_seen,
                max_bitrate: access_point.max_bitrate,
                mode: access_point.mode,
                path: path.clone(),
                rsn_flags: access_point.rsn_flags,
                ssid: access_point.ssid.clone(),
                strength: access_point.strength,
                wpa_flags: access_point.wpa_flags,
            });

            server.at(path.as_str(), access_point).await?;

            access_point_by_network_path.insert(ordered_network.path.clone(), path);
        }
    }

    let mut device_paths = vec![];
    let mut active_connection_specs = vec![];
    let mut primary_connection = root_path();
    let mut primary_connection_type = String::new();

    for link in &manager.links {
        let current_hardware_address = mac_string(&link.description.hardware_address);
        let permanent_hardware_address = mac_string(&link.description.permanent_hardware_address);
        let (driver_version, firmware_version) = ethtool_info(&link.description.name);
        let ip4_config_path = register_ip4_config(server, link).await?;
        let ip6_config_path = register_ip6_config(server, link).await?;
        let dhcp4_config_path = register_dhcp4_config(server, link).await?;
        let dhcp6_config_path = register_dhcp6_config(server, link).await?;
        let device_path = device_object_path(&link.description.name);
        let wired_settings_path = wired_settings_by_interface.get(&link.description.name).cloned();

        let (available_connections, wireless_data, settings_path) =
            if link.description.r#type == Type::Wlan {
                let station_device = wireless.device_by_name(&link.description.name);
                let station = station_device.and_then(|device| wireless.station_by_device_path(&device.path));
                let active_access_point = station
                    .and_then(|station| station.connected_network.as_ref())
                    .and_then(|network_path| access_point_by_network_path.get(network_path))
                    .cloned()
                    .unwrap_or_else(root_path);
                let access_points = station
                    .map(|station| {
                        station
                            .ordered_networks
                            .iter()
                            .filter_map(|network| {
                                access_point_by_network_path.get(&network.path).cloned()
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let bitrate = link
                    .bit_rates
                    .0
                    .max(link.bit_rates.1)
                    .saturating_div(1000)
                    .try_into()
                    .unwrap_or(u32::MAX);
                let station_path = station.map(|station| station.path.clone()).unwrap_or_else(root_path);
                let connected_settings = station
                    .and_then(|station| station.connected_network.as_ref())
                    .and_then(|network_path| wireless.network_by_path(network_path))
                    .and_then(|network| {
                        known_network_to_settings_path.get(&network.known_network).cloned()
                    });
                let last_scan = if station.is_some() { 0 } else { -1 };
                let wireless_data = DeviceWireless {
                    access_points: access_points.clone(),
                    active_access_point,
                    bitrate,
                    hw_address: current_hardware_address.clone(),
                    interface_name: link.description.name.clone(),
                    last_scan,
                    mode: NM80211Mode::Infra,
                    perm_hw_address: permanent_hardware_address.clone(),
                    runtime: runtime.clone(),
                    wireless_capabilities: wireless_capabilities(link, &access_points, &runtime),
                };
                runtime.upsert_wireless_device(WirelessDeviceRecord {
                    access_points: access_points.clone(),
                    active_access_point: wireless_data.active_access_point.clone(),
                    bitrate,
                    interface_name: link.description.name.clone(),
                    last_scan,
                });

                (
                    wifi_settings_paths.clone(),
                    Some((wireless_data, station_path)),
                    connected_settings,
                )
            } else {
                (wired_settings_path.clone().into_iter().collect(), None, wired_settings_path)
            };

        let is_active = link_is_active(link);
        let active_connection_path = if is_active && link.description.r#type != Type::Loopback {
            let path = active_connection_object_path(&link.description.name);
            if primary_connection == root_path() {
                primary_connection = path.clone();
                primary_connection_type =
                    connection_type_for_link(link).unwrap_or("generic").to_string();
            }
            Some(path)
        } else {
            None
        };

        let device_state = if is_active {
            NMDeviceState::Activated
        } else {
            NMDeviceState::Disconnected
        };

        let device = Device {
            active_connection: active_connection_path
                .clone()
                .unwrap_or_else(root_path),
            autoconnect: true,
            available_connections,
            capabilities: device_capabilities(link),
            dhcp4_config: dhcp4_config_path.clone(),
            dhcp6_config: dhcp6_config_path.clone(),
            driver: link.description.driver.clone(),
            driver_version,
            firmware_missing: false,
            firmware_version,
            hw_address: current_hardware_address.clone(),
            interface: link.description.name.clone(),
            interface_flags: interface_flags(link),
            ip4_config: ip4_config_path.clone(),
            ip4_connectivity: if link_has_ipv4(link) {
                NMConnectivityState::Full
            } else {
                NMConnectivityState::None
            },
            ip6_config: ip6_config_path.clone(),
            ip6_connectivity: if link_has_global_ipv6(link) {
                NMConnectivityState::Full
            } else {
                NMConnectivityState::None
            },
            ip_interface: link.description.name.clone(),
            lldp_neighbors: vec![],
            managed: true,
            metered: NMMetered::Unknown,
            mtu: link.description.mtu,
            nm_plugin_missing: false,
            path: link.description.name.clone(),
            physical_port_id: String::new(),
            ports: vec![],
            real: !matches!(link.description.kind, Kind::Tun | Kind::Veth | Kind::Dummy)
                && link.description.r#type != Type::Loopback,
            state: device_state,
            state_reason: (device_state, NMDeviceStateReason::None),
            r#type: device_type_for_link(link),
            runtime: runtime.clone(),
            udi: link.description.name.clone(),
        };

        server.at(device_path.as_str(), device).await?;
        register_device_aux_interfaces(server, &device_path, link).await?;

        let mut p2p_peers = Vec::new();
        match link.description.r#type {
            Type::Loopback => {
                server.at(device_path.as_str(), DeviceLoopback).await?;
            }
            Type::Ether => {
                server
                    .at(
                        device_path.as_str(),
                        DeviceWired {
                            carrier: is_active,
                            hw_address: current_hardware_address,
                            perm_hw_address: permanent_hardware_address,
                            s390_subchannels: vec![],
                            speed: wired_speed_mbps(link),
                        },
                    )
                    .await?;
            }
            Type::Wlan => {
                if let Some((wireless_data, _station_path)) = &wireless_data {
                    server
                        .at(device_path.as_str(), wireless_data.clone())
                        .await?;
                }
                if link.description.wireless_lan_interface_type == "p2p-device" {
                    let station_device = wireless.device_by_name(&link.description.name);
                    let station =
                        station_device.and_then(|device| wireless.station_by_device_path(&device.path));
                    if let Some(station) = station {
                        for ordered in &station.ordered_networks {
                            if let Some(network) = wireless.network_by_path(&ordered.path) {
                                let peer_path =
                                    wifi_p2p_peer_path(&link.description.name, &ordered.path);
                                server
                                    .at(
                                        peer_path.as_str(),
                                        WifiP2PPeer {
                                            flags: 0,
                                            hw_address: network
                                                .extended_service_set
                                                .first()
                                                .and_then(|path| wireless.basic_service_set_by_path(path))
                                                .map(|bss| bss.address.clone())
                                                .unwrap_or_default(),
                                            last_seen: 0,
                                            manufacturer: String::new(),
                                            model: String::new(),
                                            model_number: String::new(),
                                            name: network.name.clone(),
                                            serial: String::new(),
                                            strength: signal_to_strength(ordered.signal),
                                            wfd_ies: Vec::new(),
                                        },
                                    )
                                    .await?;
                                p2p_peers.push(peer_path);
                            }
                        }
                        let _ = server
                            .remove::<network_manager::device::specialized::DeviceWifiP2P, _>(
                                device_path.as_str(),
                            )
                            .await;
                        server
                            .at(
                                device_path.as_str(),
                                network_manager::device::specialized::DeviceWifiP2P {
                                    hw_address: mac_string(&link.description.hardware_address),
                                    peers: p2p_peers.clone(),
                                },
                            )
                            .await?;
                    }
                }
            }
            _ => {}
        }

        runtime.upsert_device(DeviceRecord {
            dhcp4_config: dhcp4_config_path.clone(),
            dhcp6_config: dhcp6_config_path.clone(),
            interface_name: link.description.name.clone(),
            is_ppp: link.description.name.starts_with("ppp") || link.description.r#type == Type::Ppp,
            ip4_config: ip4_config_path.clone(),
            ip6_config: ip6_config_path.clone(),
            kind: link.description.kind,
            path: device_path.clone(),
            p2p_peers,
            type_: link.description.r#type,
            wifi_p2p: link.description.wireless_lan_interface_type == "p2p-device",
        });
        device_paths.push(device_path.clone());

        if let Some(active_connection_path) = active_connection_path {
            let connection_path = settings_path.clone().unwrap_or_else(root_path);
            let is_primary = active_connection_path == primary_connection;

            active_connection_specs.push((
                active_connection_path.clone(),
                ActiveConnection {
                    connection: connection_path.clone(),
                    controller: root_path(),
                    default: is_primary,
                    default6: is_primary,
                    devices: vec![device_path],
                    dhcp4_config: dhcp4_config_path,
                    dhcp6_config: dhcp6_config_path,
                    id: active_connection_id(link, settings_path.as_ref(), &wireless),
                    ip4_config: ip4_config_path,
                    ip6_config: ip6_config_path,
                    specific_object: settings_path
                        .as_ref()
                        .and_then(|_| {
                            wireless_data.as_ref().and_then(|(wireless_data, _)| {
                                if wireless_data.active_access_point != root_path() {
                                    Some(wireless_data.active_access_point.clone())
                                } else {
                                    None
                                }
                            })
                        })
                        .unwrap_or_else(root_path),
                    state: NMActiveConnectionState::Activated,
                    state_flags: activation_state_flags(
                        link_has_ipv4(link),
                        link_has_global_ipv6(link),
                    ),
                    type_: connection_type_for_link(link).unwrap_or("generic").to_string(),
                    uuid: connection_uuid(link, settings_path.as_ref()),
                    vpn: false,
                },
            ));
        }
    }

    let active_connection_paths = active_connection_specs
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    for (path, connection) in active_connection_specs {
        let is_vpn = connection.type_ == "vpn";
        server.at(path.as_str(), connection).await?;
        if is_vpn {
            server.at(path.as_str(), VpnConnection::default()).await?;
        }
    }

    let network_manager = NetworkManager {
        active_connections: active_connection_paths.clone(),
        all_devices: device_paths.clone(),
        connectivity: if primary_connection != root_path() {
            NMConnectivityState::Full
        } else {
            NMConnectivityState::None
        },
        global_dns_configuration: HashMap::new(),
        metered: NMMetered::Unknown as u32,
        networking_enabled: true,
        permissions: permissions(),
        primary_connection,
        primary_connection_type,
        radio_flags: radio_flags(&manager, &wireless),
        startup: false,
        state: if device_paths.is_empty() {
            20
        } else if active_connection_paths.is_empty() {
            20
        } else {
            70
        },
        version: String::from("1.52.0"),
        version_info: vec![((1 << 16) | (52 << 8)), 0],
        wimax_enabled: false,
        wimax_hardware_enabled: false,
        wireless_enabled: radio_flags(&manager, &wireless) & (NMRadioFlags::WlanAvailable as u32)
            != 0,
        wireless_hardware_enabled: radio_flags(&manager, &wireless)
            & (NMRadioFlags::WlanAvailable as u32)
            != 0,
        wwan_enabled: radio_flags(&manager, &wireless) & (NMRadioFlags::WwanAvailable as u32) != 0,
        wwan_hardware_enabled: radio_flags(&manager, &wireless)
            & (NMRadioFlags::WwanAvailable as u32)
            != 0,
        runtime: runtime.clone(),
        devices: device_paths,
    };

    server.at(NM_ROOT_PATH, network_manager).await?;
    server
        .at(
            NM_AGENT_MANAGER_PATH,
            AgentManager {
                runtime: runtime.clone(),
            },
        )
        .await?;
    server
        .at(
            NM_DNS_MANAGER_PATH,
            DnsManager {
                configuration: vec![],
                mode: String::from("default"),
                rc_manager: String::from("unmanaged"),
            },
        )
        .await?;
    server.at(NM_VPN_PLUGIN_PATH, VpnPlugin::default()).await?;
    server.at("/", ActiveConnection::default()).await?;
    server.at("/", Dhcp4Config::default()).await?;
    server.at("/", Dhcp6Config::default()).await?;
    server.at("/", Ip6Config::default()).await?;
    server.at("/", Ip4Config::default()).await?;
    server.at("/org/freedesktop", ObjectManager).await?;
    server.at(NM_ROOT_PATH, ObjectManager).await?;
    server.at(NM_SETTINGS_PATH, ObjectManager).await?;

    conn.request_name("org.freedesktop.NetworkManager").await?;

    Ok((conn, runtime))
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Ip4Config {
    addresses: Vec<Vec<u32>>,
    address_data: Vec<HashMap<String, OwnedValue>>,
    dns_options: Vec<String>,
    dns_priority: i32,
    domains: Vec<String>,
    gateway: String,
    nameserver_data: Vec<HashMap<String, OwnedValue>>,
    nameservers: Vec<u32>,
    route_data: Vec<HashMap<String, OwnedValue>>,
    routes: Vec<Vec<u32>>,
    searches: Vec<String>,
    wins_server_data: Vec<String>,
    wins_servers: Vec<u32>,
}

#[interface(name = "org.freedesktop.NetworkManager.IP4Config")]
impl Ip4Config {
    #[zbus(property)]
    fn addresses(&self) -> Vec<Vec<u32>> {
        self.addresses.clone()
    }

    #[zbus(property)]
    fn address_data(&self) -> Vec<HashMap<String, OwnedValue>> {
        self.address_data.clone()
    }

    #[zbus(property)]
    fn gateway(&self) -> String {
        self.gateway.clone()
    }

    #[zbus(property)]
    fn routes(&self) -> Vec<Vec<u32>> {
        self.routes.clone()
    }

    #[zbus(property)]
    fn route_data(&self) -> Vec<HashMap<String, OwnedValue>> {
        self.route_data.clone()
    }

    #[zbus(property)]
    fn nameservers(&self) -> Vec<u32> {
        self.nameservers.clone()
    }

    #[zbus(property)]
    fn nameserver_data(&self) -> Vec<HashMap<String, OwnedValue>> {
        self.nameserver_data.clone()
    }

    #[zbus(property)]
    fn domains(&self) -> Vec<String> {
        self.domains.clone()
    }

    #[zbus(property)]
    fn searches(&self) -> Vec<String> {
        self.searches.clone()
    }

    #[zbus(property)]
    fn dns_options(&self) -> Vec<String> {
        self.dns_options.clone()
    }

    #[zbus(property)]
    fn dns_priority(&self) -> i32 {
        self.dns_priority
    }

    #[zbus(property)]
    fn wins_servers(&self) -> Vec<u32> {
        self.wins_servers.clone()
    }

    #[zbus(property)]
    fn wins_server_data(&self) -> Vec<String> {
        self.wins_server_data.clone()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Dhcp4Config {
    options: HashMap<String, OwnedValue>,
}

#[interface(name = "org.freedesktop.NetworkManager.DHCP4Config")]
impl Dhcp4Config {
    #[zbus(property)]
    fn options(&self) -> HashMap<String, OwnedValue> {
        self.options.clone()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Dhcp6Config {
    options: HashMap<String, OwnedValue>,
}

#[interface(name = "org.freedesktop.NetworkManager.DHCP6Config")]
impl Dhcp6Config {
    #[zbus(property)]
    fn options(&self) -> HashMap<String, OwnedValue> {
        self.options.clone()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Ip6Config {
    addresses: Vec<(Vec<u8>, u32, Vec<u8>)>,
    address_data: Vec<HashMap<String, OwnedValue>>,
    dns_options: Vec<String>,
    dns_priority: i32,
    domains: Vec<String>,
    gateway: String,
    nameservers: Vec<Vec<u8>>,
    route_data: Vec<HashMap<String, OwnedValue>>,
    routes: Vec<(Vec<u8>, u32, Vec<u8>, u32)>,
    searches: Vec<String>,
}

#[interface(name = "org.freedesktop.NetworkManager.IP6Config")]
impl Ip6Config {
    #[zbus(property)]
    fn addresses(&self) -> Vec<(Vec<u8>, u32, Vec<u8>)> {
        self.addresses.clone()
    }

    #[zbus(property)]
    fn address_data(&self) -> Vec<HashMap<String, OwnedValue>> {
        self.address_data.clone()
    }

    #[zbus(property)]
    fn gateway(&self) -> String {
        self.gateway.clone()
    }

    #[zbus(property)]
    fn routes(&self) -> Vec<(Vec<u8>, u32, Vec<u8>, u32)> {
        self.routes.clone()
    }

    #[zbus(property)]
    fn route_data(&self) -> Vec<HashMap<String, OwnedValue>> {
        self.route_data.clone()
    }

    #[zbus(property)]
    fn nameservers(&self) -> Vec<Vec<u8>> {
        self.nameservers.clone()
    }

    #[zbus(property)]
    fn domains(&self) -> Vec<String> {
        self.domains.clone()
    }

    #[zbus(property)]
    fn searches(&self) -> Vec<String> {
        self.searches.clone()
    }

    #[zbus(property)]
    fn dns_options(&self) -> Vec<String> {
        self.dns_options.clone()
    }

    #[zbus(property)]
    fn dns_priority(&self) -> i32 {
        self.dns_priority
    }
}

fn active_connection_id(
    link: &Link,
    settings_path: Option<&OwnedObjectPath>,
    wireless: &IwdState,
) -> String {
    if link.description.r#type == Type::Wlan {
        if let Some(settings_path) = settings_path {
            if let Some(station_device) = wireless.device_by_name(&link.description.name) {
                if let Some(station) = wireless.station_by_device_path(&station_device.path) {
                    if let Some(network_path) = &station.connected_network {
                        if let Some(network) = wireless.network_by_path(network_path) {
                            if let Some(known) = wireless.known_network_by_path(&network.known_network)
                            {
                                return known.name.clone();
                            }
                        }
                    }
                }
            }

            return settings_path
                .as_str()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
        }
    }

    link.description.name.clone()
}

fn access_point_path(network_path: &OwnedObjectPath) -> OwnedObjectPath {
    owned_path(
        format!(
            "{NM_ACCESS_POINT_PATH}/{}",
            deterministic_uuid("access-point", network_path.as_str()).replace('-', "_")
        )
        .as_str(),
    )
}

fn device_type_for_link(link: &Link) -> NMDeviceType {
    if link.description.name.starts_with("ppp") {
        return NMDeviceType::Ppp;
    }
    if link.description.wireless_lan_interface_type == "p2p-device" {
        return NMDeviceType::WifiP2P;
    }
    NMDeviceType::from((link.description.kind, link.description.r#type))
}

fn device_interface_names(kind: Kind, type_: Type, wifi_p2p: bool, is_ppp: bool) -> Vec<&'static str> {
    let mut names = vec!["org.freedesktop.NetworkManager.Device"];
    match type_ {
        Type::Loopback => names.push("org.freedesktop.NetworkManager.Device.Loopback"),
        Type::Ether => names.push("org.freedesktop.NetworkManager.Device.Wired"),
        Type::Wlan => names.push("org.freedesktop.NetworkManager.Device.Wireless"),
        _ => {}
    }
    names.extend(device_aux_interface_names(kind, type_));
    if let Some(name) = maybe_wifi_p2p_interface_name(wifi_p2p) {
        names.push(name);
    }
    names.extend(maybe_ppp_interface_names(is_ppp));
    names
}

async fn clear_config_object(server: &zbus::ObjectServer, path: &OwnedObjectPath) {
    if path == &root_path() {
        return;
    }

    if path.as_str().starts_with(NM_IP4_CONFIG_PATH) {
        let _ = server.remove::<Ip4Config, _>(path.as_str()).await;
    } else if path.as_str().starts_with(NM_IP6_CONFIG_PATH) {
        let _ = server.remove::<Ip6Config, _>(path.as_str()).await;
    } else if path.as_str().starts_with(NM_DHCP4_CONFIG_PATH) {
        let _ = server.remove::<Dhcp4Config, _>(path.as_str()).await;
    } else if path.as_str().starts_with(NM_DHCP6_CONFIG_PATH) {
        let _ = server.remove::<Dhcp6Config, _>(path.as_str()).await;
    }
}

async fn remove_config_object(
    service_bus: &Connection,
    server: &zbus::ObjectServer,
    path: &OwnedObjectPath,
) {
    if path == &root_path() {
        return;
    }
    clear_config_object(server, path).await;
    let interface_name = if path.as_str().starts_with(NM_IP4_CONFIG_PATH) {
        "org.freedesktop.NetworkManager.IP4Config"
    } else if path.as_str().starts_with(NM_IP6_CONFIG_PATH) {
        "org.freedesktop.NetworkManager.IP6Config"
    } else if path.as_str().starts_with(NM_DHCP4_CONFIG_PATH) {
        "org.freedesktop.NetworkManager.DHCP4Config"
    } else if path.as_str().starts_with(NM_DHCP6_CONFIG_PATH) {
        "org.freedesktop.NetworkManager.DHCP6Config"
    } else {
        return;
    };
    let _ = emit_object_removed(service_bus, path, interface_name).await;
}

async fn emit_added_config_if_needed(
    service_bus: &Connection,
    path: &OwnedObjectPath,
) {
    if path == &root_path() {
        return;
    }
    let interface_name = if path.as_str().starts_with(NM_IP4_CONFIG_PATH) {
        "org.freedesktop.NetworkManager.IP4Config"
    } else if path.as_str().starts_with(NM_IP6_CONFIG_PATH) {
        "org.freedesktop.NetworkManager.IP6Config"
    } else if path.as_str().starts_with(NM_DHCP4_CONFIG_PATH) {
        "org.freedesktop.NetworkManager.DHCP4Config"
    } else if path.as_str().starts_with(NM_DHCP6_CONFIG_PATH) {
        "org.freedesktop.NetworkManager.DHCP6Config"
    } else {
        return;
    };
    let _ = emit_object_added(service_bus, path, interface_name).await;
}

async fn unregister_device_objects(
    server: &zbus::ObjectServer,
    record: &DeviceRecord,
) {
    for peer in &record.p2p_peers {
        let _ = server.remove::<WifiP2PPeer, _>(peer.as_str()).await;
    }
    unregister_device_aux_interfaces(server, &record.path, record.kind, record.type_).await;
    match record.type_ {
        Type::Loopback => {
            let _ = server.remove::<DeviceLoopback, _>(record.path.as_str()).await;
        }
        Type::Ether => {
            let _ = server.remove::<DeviceWired, _>(record.path.as_str()).await;
        }
        Type::Wlan => {
            let _ = server.remove::<DeviceWireless, _>(record.path.as_str()).await;
        }
        _ => {}
    }
    let _ = server.remove::<Device, _>(record.path.as_str()).await;
    clear_config_object(server, &record.ip4_config).await;
    clear_config_object(server, &record.ip6_config).await;
    clear_config_object(server, &record.dhcp4_config).await;
    clear_config_object(server, &record.dhcp6_config).await;
}

async fn sync_device_object(
    service_bus: &Connection,
    server: &zbus::ObjectServer,
    runtime: &Runtime,
    link: &Link,
    wireless: &IwdState,
) -> Result<(OwnedObjectPath, bool, Vec<OwnedObjectPath>, Vec<OwnedObjectPath>)> {
    let device_path = device_object_path(&link.description.name);
    let existing = runtime.device(&device_path);
    if let Some(record) = &existing {
        unregister_device_objects(server, record).await;
    }

    let current_hardware_address = mac_string(&link.description.hardware_address);
    let permanent_hardware_address = mac_string(&link.description.permanent_hardware_address);
    let (driver_version, firmware_version) = ethtool_info(&link.description.name);
    let ip4_config_path = register_ip4_config(server, link).await?;
    let ip6_config_path = register_ip6_config(server, link).await?;
    let dhcp4_config_path = register_dhcp4_config(server, link).await?;
    let dhcp6_config_path = register_dhcp6_config(server, link).await?;

    if let Some(record) = &existing {
        for old_path in [
            record.ip4_config.clone(),
            record.ip6_config.clone(),
            record.dhcp4_config.clone(),
            record.dhcp6_config.clone(),
        ] {
            if ![
                &ip4_config_path,
                &ip6_config_path,
                &dhcp4_config_path,
                &dhcp6_config_path,
            ]
            .into_iter()
            .any(|candidate| candidate == &old_path)
            {
                remove_config_object(service_bus, server, &old_path).await;
            }
        }
    }

    for path in [
        ip4_config_path.clone(),
        ip6_config_path.clone(),
        dhcp4_config_path.clone(),
        dhcp6_config_path.clone(),
    ] {
        let was_present = existing.as_ref().is_some_and(|record| {
            [
                &record.ip4_config,
                &record.ip6_config,
                &record.dhcp4_config,
                &record.dhcp6_config,
            ]
            .into_iter()
            .any(|candidate| candidate == &path)
        });
        if !was_present {
            emit_added_config_if_needed(service_bus, &path).await;
        }
    }

    let available_connections = if link.description.r#type == Type::Wlan {
        runtime
            .connections()
            .into_iter()
            .filter(|record| record.connection_type == "802-11-wireless")
            .map(|record| record.path)
            .collect()
    } else {
        runtime.connections_for_interface(&link.description.name)
    };

    let active_connection = runtime
        .active_connection_for_interface(&link.description.name)
        .unwrap_or_else(root_path);
    let is_active = link_is_active(link);
    let device_state = if is_active {
        NMDeviceState::Activated
    } else {
        NMDeviceState::Disconnected
    };

    let device = Device {
        active_connection,
        autoconnect: true,
        available_connections,
        capabilities: device_capabilities(link),
        dhcp4_config: dhcp4_config_path.clone(),
        dhcp6_config: dhcp6_config_path.clone(),
        driver: link.description.driver.clone(),
        driver_version,
        firmware_missing: false,
        firmware_version,
        hw_address: current_hardware_address.clone(),
        interface: link.description.name.clone(),
        interface_flags: interface_flags(link),
        ip4_config: ip4_config_path.clone(),
        ip4_connectivity: if link_has_ipv4(link) {
            NMConnectivityState::Full
        } else {
            NMConnectivityState::None
        },
        ip6_config: ip6_config_path.clone(),
        ip6_connectivity: if link_has_global_ipv6(link) {
            NMConnectivityState::Full
        } else {
            NMConnectivityState::None
        },
        ip_interface: link.description.name.clone(),
        lldp_neighbors: vec![],
        managed: true,
        metered: NMMetered::Unknown,
        mtu: link.description.mtu,
        nm_plugin_missing: false,
        path: link.description.name.clone(),
        physical_port_id: String::new(),
        ports: vec![],
        real: !matches!(link.description.kind, Kind::Tun | Kind::Veth | Kind::Dummy)
            && link.description.r#type != Type::Loopback,
        state: device_state,
        state_reason: (device_state, NMDeviceStateReason::None),
        r#type: device_type_for_link(link),
        runtime: runtime.clone(),
        udi: link.description.name.clone(),
    };

    server.at(device_path.as_str(), device).await?;
    register_device_aux_interfaces(server, &device_path, link).await?;

    let mut p2p_peers = Vec::new();

    match link.description.r#type {
        Type::Loopback => {
            server.at(device_path.as_str(), DeviceLoopback).await?;
        }
        Type::Ether => {
            server
                .at(
                    device_path.as_str(),
                    DeviceWired {
                        carrier: is_active,
                        hw_address: current_hardware_address,
                        perm_hw_address: permanent_hardware_address,
                        s390_subchannels: vec![],
                        speed: wired_speed_mbps(link),
                    },
                )
                .await?;
        }
        Type::Wlan => {
            let station_device = wireless.device_by_name(&link.description.name);
            let station = station_device.and_then(|device| wireless.station_by_device_path(&device.path));
            let access_points = station
                .map(|station| {
                    station
                        .ordered_networks
                        .iter()
                        .map(|ordered| access_point_path(&ordered.path))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let active_access_point = station
                .and_then(|station| station.connected_network.as_ref())
                .map(access_point_path)
                .unwrap_or_else(root_path);
            let bitrate = iw_link_bitrate(&link.description.name).unwrap_or(
                link.bit_rates
                    .0
                    .max(link.bit_rates.1)
                    .saturating_div(1000)
                    .try_into()
                    .unwrap_or(u32::MAX),
            );
            let last_scan = if station.is_some() { 0 } else { -1 };
            let wireless_data = DeviceWireless {
                access_points: access_points.clone(),
                active_access_point: active_access_point.clone(),
                bitrate,
                hw_address: current_hardware_address,
                interface_name: link.description.name.clone(),
                last_scan,
                mode: NM80211Mode::Infra,
                perm_hw_address: permanent_hardware_address,
                runtime: runtime.clone(),
                wireless_capabilities: wireless_capabilities(link, &access_points, runtime),
            };
            runtime.upsert_wireless_device(WirelessDeviceRecord {
                access_points,
                active_access_point,
                bitrate,
                interface_name: link.description.name.clone(),
                last_scan,
            });
            server.at(device_path.as_str(), wireless_data).await?;
            if link.description.wireless_lan_interface_type == "p2p-device" {
                if let Some(station) = station {
                    let peers = station
                        .ordered_networks
                        .iter()
                        .filter_map(|ordered| {
                            let network = wireless.network_by_path(&ordered.path)?;
                            let peer_path = wifi_p2p_peer_path(&link.description.name, &ordered.path);
                            let hw_address = network
                                .extended_service_set
                                .first()
                                .and_then(|path| wireless.basic_service_set_by_path(path))
                                .map(|bss| bss.address.clone())
                                .unwrap_or_default();
                            Some((
                                peer_path,
                                WifiP2PPeer {
                                    flags: 0,
                                    hw_address,
                                    last_seen: 0,
                                    manufacturer: String::new(),
                                    model: String::new(),
                                    model_number: String::new(),
                                    name: network.name.clone(),
                                    serial: String::new(),
                                    strength: signal_to_strength(ordered.signal),
                                    wfd_ies: Vec::new(),
                                },
                            ))
                        })
                        .collect::<Vec<_>>();
                    for (path, peer) in peers {
                        server.at(path.as_str(), peer).await?;
                        p2p_peers.push(path);
                    }
                    let _ = server.remove::<network_manager::device::specialized::DeviceWifiP2P, _>(
                        device_path.as_str(),
                    )
                    .await;
                    server
                        .at(
                            device_path.as_str(),
                            network_manager::device::specialized::DeviceWifiP2P {
                                hw_address: mac_string(&link.description.hardware_address),
                                peers: p2p_peers.clone(),
                            },
                        )
                        .await?;
                }
            }
        }
        _ => {
            runtime.remove_wireless_device(&link.description.name);
        }
    }

    let old_peers = existing
        .as_ref()
        .map(|record| record.p2p_peers.clone())
        .unwrap_or_default();
    let removed_peers = old_peers
        .iter()
        .filter(|path| !p2p_peers.iter().any(|candidate| candidate == *path))
        .cloned()
        .collect::<Vec<_>>();
    let added_peers = p2p_peers
        .iter()
        .filter(|path| !old_peers.iter().any(|candidate| candidate == *path))
        .cloned()
        .collect::<Vec<_>>();

    runtime.upsert_device(DeviceRecord {
        dhcp4_config: dhcp4_config_path,
        dhcp6_config: dhcp6_config_path,
        interface_name: link.description.name.clone(),
        is_ppp: link.description.name.starts_with("ppp") || link.description.r#type == Type::Ppp,
        ip4_config: ip4_config_path,
        ip6_config: ip6_config_path,
        kind: link.description.kind,
        path: device_path.clone(),
        p2p_peers,
        type_: link.description.r#type,
        wifi_p2p: link.description.wireless_lan_interface_type == "p2p-device",
    });

    Ok((device_path, existing.is_none(), added_peers, removed_peers))
}

pub(crate) async fn emit_object_added(
    service_bus: &Connection,
    path: &OwnedObjectPath,
    interface_name: &str,
) -> zbus::Result<()> {
    emit_interfaces_added(service_bus, path, &[interface_name]).await
}

pub(crate) async fn emit_interfaces_added(
    service_bus: &Connection,
    path: &OwnedObjectPath,
    interface_names: &[&str],
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(service_bus, "/org/freedesktop")?;
    let interfaces = interface_names
        .iter()
        .map(|interface_name| {
            (
                InterfaceName::try_from(*interface_name).expect("interface name should be valid"),
                HashMap::<&str, Value<'static>>::new(),
            )
        })
        .collect();
    ObjectManager::interfaces_added(&emitter, path.clone().into(), interfaces).await
}

pub(crate) async fn emit_object_removed(
    service_bus: &Connection,
    path: &OwnedObjectPath,
    interface_name: &str,
) -> zbus::Result<()> {
    emit_interfaces_removed(service_bus, path, &[interface_name]).await
}

pub(crate) async fn emit_interfaces_removed(
    service_bus: &Connection,
    path: &OwnedObjectPath,
    interface_names: &[&str],
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(service_bus, "/org/freedesktop")?;
    ObjectManager::interfaces_removed(
        &emitter,
        path.clone().into(),
        interface_names
            .iter()
            .map(|interface_name| {
                InterfaceName::try_from(*interface_name).expect("interface name should be valid")
            })
            .collect::<Vec<_>>()
            .into(),
    )
    .await
}

fn build_wifi_settings(
    interface_name: &str,
    known_network: &KnownNetwork,
    uuid: &str,
) -> ConnectionSettings {
    let mut settings = HashMap::new();

    settings.insert(
        String::from("connection"),
        HashMap::from([
            (String::from("autoconnect"), owned(true)),
            (String::from("id"), owned(known_network.name.clone())),
            (
                String::from("interface-name"),
                owned(interface_name.to_string()),
            ),
            (String::from("type"), owned(String::from("802-11-wireless"))),
            (String::from("uuid"), owned(uuid.to_string())),
        ]),
    );
    settings.insert(
        String::from("802-11-wireless"),
        HashMap::from([
            (String::from("hidden"), owned(known_network.hidden)),
            (String::from("mode"), owned(String::from("infrastructure"))),
            (
                String::from("ssid"),
                OwnedValue::try_from(Value::from(known_network.name.as_bytes().to_vec()))
                    .expect("ssid bytes should fit in OwnedValue"),
            ),
        ]),
    );
    if known_network.kind != "open" {
        settings.insert(
            String::from("802-11-wireless-security"),
            HashMap::from([(String::from("key-mgmt"), owned(String::from("wpa-psk")))]),
        );
    }
    settings.insert(
        String::from("ipv4"),
        HashMap::from([(String::from("method"), owned(String::from("auto")))]),
    );
    settings.insert(
        String::from("ipv6"),
        HashMap::from([(String::from("method"), owned(String::from("auto")))]),
    );

    settings
}

fn build_link_settings(interface_name: &str, connection_type: &str, uuid: &str) -> ConnectionSettings {
    let mut settings = HashMap::from([
        (
            String::from("connection"),
            HashMap::from([
                (String::from("autoconnect"), owned(true)),
                (String::from("id"), owned(interface_name.to_string())),
                (
                    String::from("interface-name"),
                    owned(interface_name.to_string()),
                ),
                (String::from("type"), owned(connection_type.to_string())),
                (String::from("uuid"), owned(uuid.to_string())),
            ]),
        ),
        (
            String::from("ipv4"),
            HashMap::from([(String::from("method"), owned(String::from("auto")))]),
        ),
        (
            String::from("ipv6"),
            HashMap::from([(String::from("method"), owned(String::from("auto")))]),
        ),
    ]);
    settings.insert(connection_type.to_string(), HashMap::new());
    settings
}

fn build_wired_settings(interface_name: &str, uuid: &str) -> ConnectionSettings {
    build_link_settings(interface_name, "802-3-ethernet", uuid)
}

fn connection_type_for_link(link: &Link) -> Option<&'static str> {
    match link.description.kind {
        Kind::Bond => Some("bond"),
        Kind::Bridge => Some("bridge"),
        Kind::Dummy => Some("dummy"),
        Kind::Hsr => Some("hsr"),
        Kind::Geneve | Kind::Gre | Kind::Gretap | Kind::Ip6gre | Kind::Ip6gretap
        | Kind::Ip6tnl | Kind::Ipip | Kind::Sit => Some("ip-tunnel"),
        Kind::Ipvlan => Some("ipvlan"),
        Kind::Lowpan => Some("6lowpan"),
        Kind::Macsec => Some("macsec"),
        Kind::Macvlan => Some("macvlan"),
        Kind::Team => Some("team"),
        Kind::Tap | Kind::Tun => Some("tun"),
        Kind::Veth => Some("veth"),
        Kind::Vlan => Some("vlan"),
        Kind::Vrf => Some("vrf"),
        Kind::Vxlan => Some("vxlan"),
        Kind::Wireguard => Some("wireguard"),
        _ => match link.description.r#type {
            Type::Wlan => Some("802-11-wireless"),
            Type::Ether => Some("802-3-ethernet"),
            Type::Loopback => None,
            _ => None,
        },
    }
}

fn link_uuid_namespace(link: &Link) -> &'static str {
    connection_type_for_link(link).unwrap_or("link")
}

fn connection_uuid(link: &Link, settings_path: Option<&OwnedObjectPath>) -> String {
    settings_path
        .map(|path| deterministic_uuid("settings-path", path.as_str()))
        .unwrap_or_else(|| deterministic_uuid("link", &link.description.name))
}

fn current_hostname() -> String {
    fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|hostname| hostname.trim().to_string())
        .unwrap_or_default()
}

fn runtime_active_connection(
    connection: &ConnectionRecord,
    interface_name: &str,
    specific_object: OwnedObjectPath,
) -> ActiveConnection {
    ActiveConnection {
        connection: connection.path.clone(),
        controller: root_path(),
        default: true,
        default6: true,
        devices: vec![owned_path(
            format!("{NM_DEVICES_PATH}/{}", object_path_segment(interface_name)).as_str(),
        )],
        dhcp4_config: root_path(),
        dhcp6_config: root_path(),
        id: connection.id(),
        ip4_config: root_path(),
        ip6_config: root_path(),
        specific_object,
        state: NMActiveConnectionState::Activated,
        state_flags: activation_state_flags(true, false),
        type_: connection.connection_type.clone(),
        uuid: connection.uuid.clone(),
        vpn: connection.connection_type == "vpn",
    }
}

fn sync_disabled() -> bool {
    !crate::config::current().sync_enabled
}

fn activation_state_flags(ip4_ready: bool, ip6_ready: bool) -> u32 {
    let mut flags = NMActivationStateFlags::Layer2Ready as u32;
    if ip4_ready {
        flags |= NMActivationStateFlags::Ip4Ready as u32;
    }
    if ip6_ready {
        flags |= NMActivationStateFlags::Ip6Ready as u32;
    }
    flags
}

fn sync_interval() -> Duration {
    crate::config::current().sync_interval
}

pub fn spawn_sync_task(service_bus: Connection, runtime: Runtime) {
    if sync_disabled() {
        return;
    }

    tokio::spawn(async move {
        let system_bus = match Builder::system() {
            Ok(builder) => match builder.build().await {
                Ok(connection) => connection,
                Err(_) => return,
            },
            Err(_) => return,
        };

        if let Ok(mut streams) = signal_streams(&system_bus).await {
            let service_bus = service_bus.clone();
            let runtime = runtime.clone();
            let system_bus = system_bus.clone();
            tokio::spawn(async move {
                while let Some(message) = streams.next().await {
                    if message.is_err() {
                        continue;
                    }
                    let Ok(manager) = Manager::request(&system_bus).await else {
                        continue;
                    };
                    let wireless = IwdState::request(&system_bus).await.unwrap_or_default();
                    let _ = sync_backends(&service_bus, &runtime, manager, wireless).await;
                }
            });
        }

        loop {
            sleep(sync_interval()).await;
            let Ok(manager) = Manager::request(&system_bus).await else {
                continue;
            };
            let wireless = IwdState::request(&system_bus).await.unwrap_or_default();
            let _ = sync_backends(&service_bus, &runtime, manager, wireless).await;
        }
    });
}

pub async fn sync_backends(
    service_bus: &Connection,
    runtime: &Runtime,
    manager: Manager,
    wireless: IwdState,
) -> Result<()> {
    let server = service_bus.object_server();
    let wireless_interface_name = manager
        .links
        .iter()
        .find(|link| link.description.r#type == Type::Wlan)
        .map(|link| link.description.name.clone())
        .unwrap_or_default();

    let mut desired_backend_uuids = Vec::new();
    for known_network in &wireless.known_networks {
        let uuid = deterministic_uuid("wifi", &known_network.path.to_string());
        desired_backend_uuids.push(uuid.clone());
        let settings = build_wifi_settings(&wireless_interface_name, known_network, &uuid);
        let filename = synthetic_iwd_filename(known_network);
        if let Some(existing) = runtime.connection_by_uuid(&uuid) {
            let _ = runtime.update_connection(&existing.path, |record| {
                record.connection_type = String::from("802-11-wireless");
                record.filename = filename.clone();
                record.origin = runtime::ConnectionOrigin::BackendWifi;
                record.settings = settings.clone();
                record.unsaved = false;
            });
        } else {
            let path = runtime.next_connection_path();
            runtime.add_connection(ConnectionRecord {
                connection_type: String::from("802-11-wireless"),
                filename,
                flags: 0,
                origin: runtime::ConnectionOrigin::BackendWifi,
                path: path.clone(),
                settings,
                unsaved: false,
                uuid,
            });
            let _ = server
                .at(
                    path.as_str(),
                    SettingsConnection {
                        path: path.clone(),
                        runtime: runtime.clone(),
                    },
                )
                .await;
            let _ = emit_object_added(
                service_bus,
                &path,
                "org.freedesktop.NetworkManager.Settings.Connection",
            )
            .await;
        }
    }

    for link in manager.links.iter() {
        let Some(connection_type) = connection_type_for_link(link) else {
            continue;
        };
        if connection_type == "802-11-wireless" {
            continue;
        }

        let uuid = deterministic_uuid(link_uuid_namespace(link), &link.description.name);
        desired_backend_uuids.push(uuid.clone());
        let settings = build_link_settings(&link.description.name, connection_type, &uuid);
        let filename = synthetic_networkd_filename(link, connection_type);
        if let Some(existing) = runtime.connection_by_uuid(&uuid) {
            let _ = runtime.update_connection(&existing.path, |record| {
                record.connection_type = String::from(connection_type);
                record.filename = filename.clone();
                record.origin = runtime::ConnectionOrigin::BackendWired;
                record.settings = settings.clone();
                record.unsaved = false;
            });
        } else {
            let path = runtime.next_connection_path();
            runtime.add_connection(ConnectionRecord {
                connection_type: String::from(connection_type),
                filename,
                flags: 0,
                origin: runtime::ConnectionOrigin::BackendWired,
                path: path.clone(),
                settings,
                unsaved: false,
                uuid,
            });
            let _ = server
                .at(
                    path.as_str(),
                    SettingsConnection {
                        path: path.clone(),
                        runtime: runtime.clone(),
                    },
                )
                .await;
            let _ = emit_object_added(
                service_bus,
                &path,
                "org.freedesktop.NetworkManager.Settings.Connection",
            )
            .await;
        }
    }

    let stale_backend = runtime
        .connections()
        .into_iter()
        .filter(|connection| {
            matches!(
                connection.origin,
                runtime::ConnectionOrigin::BackendWifi | runtime::ConnectionOrigin::BackendWired
            ) && !desired_backend_uuids.iter().any(|uuid| uuid == &connection.uuid)
        })
        .map(|connection| connection.path)
        .collect::<Vec<_>>();
    for path in stale_backend {
        runtime.remove_connection(&path);
        let _ = server
            .remove::<SettingsConnection, _>(path.as_str())
            .await;
        let _ = emit_object_removed(
            service_bus,
            &path,
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await;
    }

    let mut desired_ap_paths = Vec::new();
    let mut scan_cache = HashMap::new();
    for station in &wireless.stations {
        for ordered_network in &station.ordered_networks {
            let Some(network) = wireless.network_by_path(&ordered_network.path) else {
                continue;
            };
            let hardware_address = network
                .extended_service_set
                .first()
                .and_then(|path| wireless.basic_service_set_by_path(path))
                .map(|bss| bss.address.clone())
                .unwrap_or_default();
            let interface_name = wireless
                .devices
                .iter()
                .find(|device| device.path == network.device)
                .map(|device| device.name.clone())
                .unwrap_or_default();
            let metadata = scan_cache
                .entry(interface_name.clone())
                .or_insert_with_key(|interface_name| iw_scan_metadata(interface_name))
                .get(&hardware_address.to_ascii_lowercase())
                .cloned()
                .unwrap_or_default();
            let (flags, wpa_flags, rsn_flags) = ap_security_flags(network.kind.as_str());
            let path = access_point_path(&ordered_network.path);
            desired_ap_paths.push(path.clone());
            let record = AccessPointRecord {
                bandwidth: metadata.bandwidth.max(20),
                flags: metadata.flags | flags,
                frequency: metadata.frequency,
                hw_address: hardware_address,
                last_seen: metadata.last_seen,
                max_bitrate: metadata.max_bitrate,
                mode: NM80211Mode::Infra,
                path: path.clone(),
                rsn_flags: metadata.rsn_flags | rsn_flags,
                ssid: network.name.clone(),
                strength: signal_to_strength(ordered_network.signal),
                wpa_flags: metadata.wpa_flags | wpa_flags,
            };
            let is_new = runtime.access_point(&path).is_none();
            runtime.upsert_access_point(record.clone());
            let _ = server.remove::<AccessPoint, _>(path.as_str()).await;
            let _ = server
                .at(
                    path.as_str(),
                    AccessPoint {
                        bandwidth: record.bandwidth,
                        flags: record.flags,
                        frequency: record.frequency,
                        hw_address: record.hw_address.clone(),
                        last_seen: record.last_seen,
                        max_bitrate: record.max_bitrate,
                        mode: record.mode,
                        path: path.clone(),
                        rsn_flags: record.rsn_flags,
                        runtime: runtime.clone(),
                        ssid: record.ssid.clone(),
                        strength: record.strength,
                        wpa_flags: record.wpa_flags,
                    },
                )
                .await;
            if is_new {
                let _ = emit_object_added(
                    service_bus,
                    &path,
                    "org.freedesktop.NetworkManager.AccessPoint",
                )
                .await;
                let _ = crate::network_manager::device::wireless::emit_access_point_added_signal(
                    service_bus,
                    &interface_name,
                    path.clone(),
                )
                .await;
            }
        }
    }
    for path in runtime.access_point_paths() {
        if desired_ap_paths.iter().any(|candidate| candidate == &path) {
            continue;
        }
        let interface_name = runtime.interface_for_access_point(&path);
        runtime.remove_access_point(&path);
        let _ = server.remove::<AccessPoint, _>(path.as_str()).await;
        if let Some(interface_name) = interface_name {
            let _ = crate::network_manager::device::wireless::emit_access_point_removed_signal(
                service_bus,
                &interface_name,
                path.clone(),
            )
            .await;
        }
        let _ = emit_object_removed(
            service_bus,
            &path,
            "org.freedesktop.NetworkManager.AccessPoint",
        )
        .await;
    }

    let desired_device_paths = manager
        .links
        .iter()
        .map(|link| device_object_path(&link.description.name))
        .collect::<Vec<_>>();
    let root_emitter = SignalEmitter::new(service_bus, NM_ROOT_PATH)?;
    for path in runtime.device_paths() {
        if desired_device_paths.iter().any(|candidate| candidate == &path) {
            continue;
        }
        if let Some(record) = runtime.remove_device(&path) {
            let interface_names =
                device_interface_names(record.kind, record.type_, record.wifi_p2p, record.is_ppp);
            if record.wifi_p2p {
                let device_emitter = SignalEmitter::new(service_bus, &path)?;
                for peer in &record.p2p_peers {
                    let _ = emit_object_removed(
                        service_bus,
                        peer,
                        "org.freedesktop.NetworkManager.WifiP2PPeer",
                    )
                    .await;
                    let _ = network_manager::device::specialized::DeviceWifiP2P::emit_peer_removed(
                        &device_emitter,
                        peer.clone(),
                    )
                    .await;
                }
            }
            unregister_device_objects(server, &record).await;
            runtime.remove_wireless_device(&record.interface_name);
            remove_config_object(service_bus, server, &record.ip4_config).await;
            remove_config_object(service_bus, server, &record.ip6_config).await;
            remove_config_object(service_bus, server, &record.dhcp4_config).await;
            remove_config_object(service_bus, server, &record.dhcp6_config).await;
            let _ = emit_interfaces_removed(service_bus, &path, &interface_names).await;
            let _ = NetworkManager::emit_device_removed(&root_emitter, path.clone()).await;
        }
    }
    for link in &manager.links {
        let (path, is_new, added_peers, removed_peers) =
            sync_device_object(service_bus, server, runtime, link, &wireless).await?;
        if is_new {
            let interface_names = device_interface_names(
                link.description.kind,
                link.description.r#type,
                link.description.wireless_lan_interface_type == "p2p-device",
                link.description.name.starts_with("ppp"),
            );
            let _ = emit_interfaces_added(service_bus, &path, &interface_names).await;
            let _ = NetworkManager::emit_device_added(&root_emitter, path.clone()).await;
        }
        if link.description.wireless_lan_interface_type == "p2p-device" {
            let device_emitter = SignalEmitter::new(service_bus, &path)?;
            for peer in removed_peers {
                let _ = emit_object_removed(
                    service_bus,
                    &peer,
                    "org.freedesktop.NetworkManager.WifiP2PPeer",
                )
                .await;
                let _ = network_manager::device::specialized::DeviceWifiP2P::emit_peer_removed(
                    &device_emitter,
                    peer,
                )
                .await;
            }
            for peer in added_peers {
                let _ = emit_object_added(
                    service_bus,
                    &peer,
                    "org.freedesktop.NetworkManager.WifiP2PPeer",
                )
                .await;
                let _ = network_manager::device::specialized::DeviceWifiP2P::emit_peer_added(
                    &device_emitter,
                    peer,
                )
                .await;
            }
        }
    }

    let desired_active = manager
        .links
        .iter()
        .filter(|link| link.description.r#type != Type::Loopback)
        .filter(|link| link_is_active(link))
        .filter_map(|link| {
            let connection = if link.description.r#type == Type::Wlan {
                wireless
                    .device_by_name(&link.description.name)
                    .and_then(|device| wireless.station_by_device_path(&device.path))
                    .and_then(|station| station.connected_network.as_ref())
                    .and_then(|network_path| wireless.network_by_path(network_path))
                    .and_then(|network| runtime.connection_by_uuid(&deterministic_uuid("wifi", network.known_network.as_str())))
            } else {
                runtime.connection_by_uuid(&deterministic_uuid(link_uuid_namespace(link), &link.description.name))
            }?;
            Some((
                active_connection_object_path(&link.description.name),
                runtime_active_connection(&connection, &link.description.name, root_path()),
            ))
        })
        .collect::<Vec<_>>();

    for link in manager
        .links
        .iter()
        .filter(|link| link.description.r#type == Type::Wlan)
    {
        let station_device = wireless.device_by_name(&link.description.name);
        let station = station_device.and_then(|device| wireless.station_by_device_path(&device.path));
        let active_access_point = station
            .and_then(|station| station.connected_network.as_ref())
            .map(access_point_path)
            .unwrap_or_else(root_path);
        let access_points = station
            .map(|station| station.ordered_networks.iter().map(|ordered| access_point_path(&ordered.path)).collect())
            .unwrap_or_default();
        let bitrate = iw_link_bitrate(&link.description.name).unwrap_or(
            link.bit_rates
                .0
                .max(link.bit_rates.1)
                .saturating_div(1000)
                .try_into()
                .unwrap_or(u32::MAX),
        );
        runtime.upsert_wireless_device(WirelessDeviceRecord {
            access_points,
            active_access_point,
            bitrate,
            interface_name: link.description.name.clone(),
            last_scan: if station.is_some() { 0 } else { -1 },
        });
    }

    let desired_paths = desired_active
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    for active_path in runtime.active_connection_paths() {
        if desired_paths.iter().any(|path| path == &active_path) {
            continue;
        }
        runtime.remove_active_connection(&active_path);
        let _ = server
            .remove::<ActiveConnection, _>(active_path.as_str())
            .await;
        let _ = server
            .remove::<VpnConnection, _>(active_path.as_str())
            .await;
        let _ = emit_object_removed(
            service_bus,
            &active_path,
            "org.freedesktop.NetworkManager.Connection.Active",
        )
        .await;
    }
    for (path, active) in desired_active {
        let is_new = runtime.active_connection(&path).is_none();
        let _ = server.remove::<ActiveConnection, _>(path.as_str()).await;
        let _ = server.remove::<VpnConnection, _>(path.as_str()).await;
        server.at(path.as_str(), active.clone()).await?;
        if active.type_ == "vpn" {
            server.at(path.as_str(), VpnConnection::default()).await?;
        }
        runtime.add_active_connection(ActiveConnectionRecord {
            path: path.clone(),
            value: active,
        });
        if is_new {
            let _ = emit_object_added(
                service_bus,
                &path,
                "org.freedesktop.NetworkManager.Connection.Active",
            )
            .await;
        }
    }

    let _ = crate::network_manager::settings::emit_settings_changed(service_bus, runtime, None).await;
    let root_changed = HashMap::from([
        ("ActiveConnections", Value::from(runtime.active_connection_paths())),
        (
            "Connectivity",
            Value::from(
                if runtime.sleeping()
                    || !runtime.networking_enabled()
                    || runtime.active_connection_paths().is_empty()
                {
                    NMConnectivityState::None as u32
                } else {
                    NMConnectivityState::Full as u32
                },
            ),
        ),
        ("Checkpoints", Value::from(runtime.checkpoint_paths())),
    ]);
    let emitter = SignalEmitter::new(service_bus, "/org/freedesktop/NetworkManager")?;
    Properties::properties_changed(
        &emitter,
        InterfaceName::try_from("org.freedesktop.NetworkManager")
            .expect("root interface name should be valid"),
        root_changed,
        Vec::<&str>::new().into(),
    )
    .await?;

    Ok(())
}

async fn signal_streams(system_bus: &Connection) -> zbus::Result<SelectAll<MessageStream>> {
    let mut streams = SelectAll::new();
    for rule in [
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender("net.connman.iwd")?
            .interface("org.freedesktop.DBus.ObjectManager")?
            .member("InterfacesAdded")?
            .build(),
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender("net.connman.iwd")?
            .interface("org.freedesktop.DBus.ObjectManager")?
            .member("InterfacesRemoved")?
            .build(),
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender("net.connman.iwd")?
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .build(),
        MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender("org.freedesktop.network1")?
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .build(),
    ] {
        streams.push(MessageStream::for_match_rule(rule, system_bus, Some(32)).await?);
    }
    Ok(streams)
}

#[derive(Clone, Debug, Default)]
struct IwAccessPointMetadata {
    bandwidth: u32,
    flags: u32,
    frequency: u32,
    last_seen: i32,
    max_bitrate: u32,
    rsn_flags: u32,
    wpa_flags: u32,
}

fn deterministic_uuid(namespace: &str, value: &str) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("nm-dbus-proxy:{namespace}:{value}").as_bytes(),
    )
    .to_string()
}

fn iw_bin() -> String {
    crate::config::current().iw_bin
}

fn iw_link_bitrate(interface_name: &str) -> Option<u32> {
    let output = std::process::Command::new(iw_bin())
        .args(["dev", interface_name, "link"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("tx bitrate:") {
            let rate = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<f64>().ok())?;
            return Some((rate * 1000.0).round() as u32);
        }
    }

    None
}

fn iw_scan_metadata(interface_name: &str) -> HashMap<String, IwAccessPointMetadata> {
    let Ok(output) = std::process::Command::new(iw_bin())
        .args(["dev", interface_name, "scan", "dump"])
        .output()
    else {
        return HashMap::new();
    };
    if !output.status.success() {
        return HashMap::new();
    }

    let mut out = HashMap::new();
    let mut current_bssid = String::new();
    let mut current = IwAccessPointMetadata::default();

    let flush = |out: &mut HashMap<String, IwAccessPointMetadata>,
                 current_bssid: &mut String,
                 current: &mut IwAccessPointMetadata| {
        if !current_bssid.is_empty() {
            out.insert(current_bssid.clone(), current.clone());
            *current_bssid = String::new();
            *current = IwAccessPointMetadata::default();
        }
    };

    for raw_line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("BSS ") {
            flush(&mut out, &mut current_bssid, &mut current);
            current_bssid = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .split('(')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            continue;
        }
        if current_bssid.is_empty() {
            continue;
        }

        if let Some(value) = line.strip_prefix("freq:") {
            current.frequency = value.trim().parse::<u32>().unwrap_or_default();
        } else if let Some(value) = line.strip_prefix("last seen:") {
            current.last_seen = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<i32>().ok())
                .map(|value| -value)
                .unwrap_or(-1);
        } else if let Some(value) = line.strip_prefix("* channel width:") {
            current.bandwidth = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(current.bandwidth);
        } else if let Some(value) = line.strip_prefix("max bitrate:") {
            current.max_bitrate = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<f64>().ok())
                .map(|value| (value * 1000.0).round() as u32)
                .unwrap_or(current.max_bitrate);
        } else if line == "RSN:" {
            current.flags = 1;
            current.rsn_flags = 1;
        } else if line == "WPA:" {
            current.flags = 1;
            current.wpa_flags = 1;
        }
    }

    flush(&mut out, &mut current_bssid, &mut current);
    out
}

fn ap_security_flags(kind: &str) -> (u32, u32, u32) {
    match kind {
        "open" => (NM80211ApFlags::None as u32, 0, 0),
        "psk" => (
            NM80211ApFlags::Privacy as u32,
            (NM80211ApSecurityFlags::KeyMgmtPsk as u32)
                | (NM80211ApSecurityFlags::PairCcmp as u32)
                | (NM80211ApSecurityFlags::GroupCcmp as u32),
            (NM80211ApSecurityFlags::KeyMgmtPsk as u32)
                | (NM80211ApSecurityFlags::PairCcmp as u32)
                | (NM80211ApSecurityFlags::GroupCcmp as u32)
                | (NM80211ApSecurityFlags::KeyMgmtSae as u32),
        ),
        _ => (NM80211ApFlags::Privacy as u32, 0, 0),
    }
}

fn device_capabilities(link: &Link) -> u32 {
    let mut capabilities = NMDeviceCapabilities::NMSupported as u32;

    if matches!(link.description.r#type, Type::Ether | Type::Wlan | Type::Wwan) {
        capabilities |= NMDeviceCapabilities::CarrierDetect as u32;
    }
    if matches!(
        link.description.kind,
        crate::systemd_networkd::link::Kind::Tun | crate::systemd_networkd::link::Kind::Veth
    ) || matches!(link.description.r#type, Type::Loopback)
    {
        capabilities |= NMDeviceCapabilities::IsSoftware as u32;
    }

    capabilities
}

fn wireless_capabilities(
    link: &Link,
    access_points: &[OwnedObjectPath],
    runtime: &Runtime,
) -> u32 {
    let mut capabilities = (NMDeviceWifiCapabilities::FreqValid as u32)
        | (NMDeviceWifiCapabilities::Freq2Ghz as u32);

    if matches!(link.description.wireless_lan_interface_type.as_str(), "station" | "ap") {
        capabilities |= NMDeviceWifiCapabilities::Ap as u32;
    }
    if access_points.iter().any(|path| {
        runtime
            .access_point(path)
            .map(|record| record.frequency >= 5000)
            .unwrap_or(false)
    }) {
        capabilities |= NMDeviceWifiCapabilities::Freq5Ghz as u32;
    }
    if access_points.iter().any(|path| {
        runtime
            .access_point(path)
            .map(|record| record.frequency >= 5925)
            .unwrap_or(false)
    }) {
        capabilities |= NMDeviceWifiCapabilities::Freq6Ghz as u32;
    }
    if access_points.iter().any(|path| {
        runtime
            .access_point(path)
            .map(|record| record.wpa_flags != 0 || record.rsn_flags != 0)
            .unwrap_or(false)
    }) {
        capabilities |= (NMDeviceWifiCapabilities::Wpa as u32)
            | (NMDeviceWifiCapabilities::Rsn as u32)
            | (NMDeviceWifiCapabilities::CipherCcmp as u32);
    }

    capabilities
}

fn interface_flags(link: &Link) -> NMDeviceInterfaceFlags {
    if link.carrier_state == "carrier" {
        NMDeviceInterfaceFlags::Carrier
    } else if link.administrative_state == "configured" {
        NMDeviceInterfaceFlags::Up
    } else {
        NMDeviceInterfaceFlags::None
    }
}

fn link_has_global_ipv6(link: &Link) -> bool {
    link.description
        .addresses
        .iter()
        .any(|address| address.family == 10 && !address.address_string.starts_with("fe80:"))
}

fn link_has_ipv4(link: &Link) -> bool {
    link.description.addresses.iter().any(|address| address.family == 2)
}

fn wired_speed_mbps(link: &Link) -> u32 {
    link.bit_rates
        .0
        .max(link.bit_rates.1)
        .saturating_div(1_000_000)
        .try_into()
        .unwrap_or(0)
}

fn link_is_active(link: &Link) -> bool {
    !matches!(
        link.operational_state.as_str(),
        "off" | "down" | "lower-layer-down" | "no-carrier" | "dormant"
    ) && !matches!(link.carrier_state.as_str(), "off" | "no-carrier")
}

fn mac_string(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn owned<T>(value: T) -> OwnedValue
where
    T: Into<Value<'static>>,
{
    OwnedValue::try_from(value.into()).expect("value should fit in OwnedValue")
}

fn owned_path(path: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(path).expect("generated object path should be valid")
}

fn permissions() -> HashMap<String, String> {
    [
        "org.freedesktop.NetworkManager.enable-disable-network",
        "org.freedesktop.NetworkManager.enable-disable-wifi",
        "org.freedesktop.NetworkManager.enable-disable-wwan",
        "org.freedesktop.NetworkManager.enable-disable-wimax",
        "org.freedesktop.NetworkManager.sleep-wake",
        "org.freedesktop.NetworkManager.network-control",
        "org.freedesktop.NetworkManager.wifi.share.protected",
        "org.freedesktop.NetworkManager.wifi.share.open",
        "org.freedesktop.NetworkManager.settings.modify.system",
        "org.freedesktop.NetworkManager.settings.modify.own",
        "org.freedesktop.NetworkManager.settings.modify.hostname",
        "org.freedesktop.NetworkManager.settings.modify.global-dns",
        "org.freedesktop.NetworkManager.reload",
        "org.freedesktop.NetworkManager.checkpoint-rollback",
        "org.freedesktop.NetworkManager.enable-disable-statistics",
        "org.freedesktop.NetworkManager.enable-disable-connectivity-check",
        "org.freedesktop.NetworkManager.wifi.scan",
    ]
    .into_iter()
    .map(|permission| (String::from(permission), String::from("yes")))
    .collect()
}

fn radio_flags(manager: &Manager, wireless: &IwdState) -> u32 {
    let mut flags = 0u32;

    if wireless.devices.iter().any(|device| device.powered) {
        flags |= NMRadioFlags::WlanAvailable as u32;
    }
    if manager
        .links
        .iter()
        .any(|link| link.description.r#type == Type::Wwan)
    {
        flags |= NMRadioFlags::WwanAvailable as u32;
    }

    flags
}

async fn register_ip4_config(
    server: &zbus::ObjectServer,
    link: &Link,
) -> Result<OwnedObjectPath> {
    let Some(config) = ip4_config(link) else {
        return Ok(root_path());
    };

    let path = config_object_path(NM_IP4_CONFIG_PATH, &link.description.name);
    let _ = server.remove::<Ip4Config, _>(path.as_str()).await;
    server.at(path.as_str(), config).await?;

    Ok(path)
}

async fn register_ip6_config(
    server: &zbus::ObjectServer,
    link: &Link,
) -> Result<OwnedObjectPath> {
    let Some(config) = ip6_config(link) else {
        return Ok(root_path());
    };

    let path = config_object_path(NM_IP6_CONFIG_PATH, &link.description.name);
    let _ = server.remove::<Ip6Config, _>(path.as_str()).await;
    server.at(path.as_str(), config).await?;

    Ok(path)
}

async fn register_dhcp4_config(
    server: &zbus::ObjectServer,
    link: &Link,
) -> Result<OwnedObjectPath> {
    let Some(config) = dhcp4_config(link) else {
        return Ok(root_path());
    };

    let path = config_object_path(NM_DHCP4_CONFIG_PATH, &link.description.name);
    let _ = server.remove::<Dhcp4Config, _>(path.as_str()).await;
    server.at(path.as_str(), config).await?;

    Ok(path)
}

async fn register_dhcp6_config(
    server: &zbus::ObjectServer,
    link: &Link,
) -> Result<OwnedObjectPath> {
    let Some(config) = dhcp6_config(link) else {
        return Ok(root_path());
    };

    let path = config_object_path(NM_DHCP6_CONFIG_PATH, &link.description.name);
    let _ = server.remove::<Dhcp6Config, _>(path.as_str()).await;
    server.at(path.as_str(), config).await?;

    Ok(path)
}

fn root_path() -> OwnedObjectPath {
    owned_path("/")
}

fn signal_to_strength(signal: i16) -> u8 {
    let dbm = f64::from(signal) / 100.0;
    let strength = ((dbm + 100.0) * 2.0).clamp(0.0, 100.0);

    strength.round() as u8
}

fn synthetic_iwd_filename(known_network: &KnownNetwork) -> String {
    let suffix = match known_network.kind.as_str() {
        "open" => "open",
        "psk" => "psk",
        kind => kind,
    };

    format!("/var/lib/iwd/{}.{}", known_network.name, suffix)
}

fn synthetic_networkd_filename(link: &Link, connection_type: &str) -> String {
    let extension = if connection_type == "802-3-ethernet" {
        "network"
    } else {
        "netdev"
    };
    crate::config::current()
        .network_dir
        .join(format!("{}.{}", link.description.name, extension))
        .display()
        .to_string()
}

fn ethtool_info(interface_name: &str) -> (String, String) {
    let Ok(output) = std::process::Command::new("ethtool")
        .args(["-i", interface_name])
        .output()
    else {
        return (String::new(), String::new());
    };
    if !output.status.success() {
        return (String::new(), String::new());
    }

    let mut driver_version = String::new();
    let mut firmware_version = String::new();
    for raw_line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = raw_line.trim();
        if let Some(value) = line.strip_prefix("version:") {
            driver_version = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("firmware-version:") {
            firmware_version = value.trim().to_string();
        }
    }

    (driver_version, firmware_version)
}

fn ip4_address_data(address: &Address) -> HashMap<String, OwnedValue> {
    HashMap::from([
        (
            String::from("address"),
            owned(address.address_string.clone()),
        ),
        (
            String::from("prefix"),
            owned(u32::from(address.prefix_length)),
        ),
    ])
}

fn ip4_config(link: &Link) -> Option<Ip4Config> {
    let ipv4_addresses = link
        .description
        .addresses
        .iter()
        .filter(|address| address.family == 2)
        .collect::<Vec<_>>();
    let nameservers = crate::network_manager::dns_manager::current_nameservers()
        .into_iter()
        .filter_map(|value| ipv4_to_u32(&value))
        .collect::<Vec<_>>();
    let nameserver_data = nameservers
        .iter()
        .copied()
        .map(|address| {
            HashMap::from([(String::from("address"), owned(u32_to_ipv4(address)))])
        })
        .collect::<Vec<_>>();
    let domains = crate::network_manager::dns_manager::current_domains();

    if ipv4_addresses.is_empty() && nameservers.is_empty() && domains.is_empty() {
        return None;
    }

    Some(Ip4Config {
        addresses: ipv4_addresses
            .iter()
            .filter_map(|address| {
                ipv4_to_u32(&address.address_string)
                    .map(|value| vec![value, u32::from(address.prefix_length), 0])
            })
            .collect(),
        address_data: ipv4_addresses.iter().map(|address| ip4_address_data(address)).collect(),
        dns_options: Vec::new(),
        dns_priority: 0,
        domains: domains.clone(),
        gateway: String::new(),
        nameserver_data,
        nameservers,
        route_data: Vec::new(),
        routes: Vec::new(),
        searches: domains,
        wins_server_data: Vec::new(),
        wins_servers: Vec::new(),
    })
}

fn ip6_config(link: &Link) -> Option<Ip6Config> {
    let ipv6_addresses = link
        .description
        .addresses
        .iter()
        .filter(|address| address.family == 10)
        .collect::<Vec<_>>();
    let nameservers = crate::network_manager::dns_manager::current_nameservers()
        .into_iter()
        .filter_map(|value| ipv6_to_bytes(&value))
        .collect::<Vec<_>>();
    let domains = crate::network_manager::dns_manager::current_domains();

    if ipv6_addresses.is_empty() && nameservers.is_empty() && domains.is_empty() {
        return None;
    }

    Some(Ip6Config {
        addresses: ipv6_addresses
            .iter()
            .filter_map(|address| {
                ipv6_to_bytes(&address.address_string).map(|value| {
                    (value, u32::from(address.prefix_length), Vec::new())
                })
            })
            .collect(),
        address_data: ipv6_addresses.iter().map(|address| ip6_address_data(address)).collect(),
        dns_options: Vec::new(),
        dns_priority: 0,
        domains: domains.clone(),
        gateway: String::new(),
        nameservers,
        route_data: Vec::new(),
        routes: Vec::new(),
        searches: domains,
    })
}

fn dhcp4_config(link: &Link) -> Option<Dhcp4Config> {
    if !link_has_ipv4(link) {
        return None;
    }

    let nameservers = crate::network_manager::dns_manager::current_nameservers()
        .into_iter()
        .filter(|value| value.parse::<std::net::Ipv4Addr>().is_ok())
        .collect::<Vec<_>>();
    let domains = crate::network_manager::dns_manager::current_domains();
    let mut options = HashMap::from([
        (String::from("interface-mtu"), owned(link.description.mtu)),
        (String::from("host-name"), owned(current_hostname())),
    ]);
    if !nameservers.is_empty() {
        options.insert(
            String::from("domain_name_servers"),
            owned(nameservers.join(" ")),
        );
    }
    if let Some(domain) = domains.first() {
        options.insert(String::from("domain_name"), owned(domain.clone()));
    }

    Some(Dhcp4Config { options })
}

fn dhcp6_config(link: &Link) -> Option<Dhcp6Config> {
    if !link_has_global_ipv6(link) {
        return None;
    }

    Some(Dhcp6Config {
        options: HashMap::from([
            (String::from("interface-mtu"), owned(link.description.mtu)),
            (String::from("host-name"), owned(current_hostname())),
        ]),
    })
}

fn ip6_address_data(address: &Address) -> HashMap<String, OwnedValue> {
    HashMap::from([
        (
            String::from("address"),
            owned(address.address_string.clone()),
        ),
        (
            String::from("prefix"),
            owned(u32::from(address.prefix_length)),
        ),
    ])
}

fn ipv4_to_u32(value: &str) -> Option<u32> {
    let addr = value.parse::<std::net::Ipv4Addr>().ok()?;
    Some(u32::from_be_bytes(addr.octets()))
}

fn u32_to_ipv4(value: u32) -> String {
    std::net::Ipv4Addr::from(value.to_be_bytes()).to_string()
}

fn ipv6_to_bytes(value: &str) -> Option<Vec<u8>> {
    let addr = value.parse::<std::net::Ipv6Addr>().ok()?;
    Some(addr.octets().to_vec())
}
