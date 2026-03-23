use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use zbus::zvariant::OwnedObjectPath;
use zbus::{
    fdo, interface,
    object_server::{ObjectServer, SignalEmitter},
};

use crate::systemd_networkd::link::{Kind, Link, Type};

fn hw_address(link: &Link) -> String {
    link.description
        .hardware_address
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn synthetic_ports(path: &OwnedObjectPath) -> Vec<OwnedObjectPath> {
    vec![path.clone()]
}

macro_rules! empty_device_interface {
    ($name:ident, $iface:literal) => {
        #[derive(Clone, Debug, Default, Eq, PartialEq)]
        pub struct $name;

        #[interface(name = $iface)]
        impl $name {}
    };
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceCarrierSlaves {
    pub carrier: bool,
    pub hw_address: String,
    pub slaves: Vec<OwnedObjectPath>,
}

macro_rules! carrier_slaves_interface {
    ($name:ident, $iface:literal) => {
        #[derive(Clone, Debug, Default, Eq, PartialEq)]
        pub struct $name(pub DeviceCarrierSlaves);

        #[interface(name = $iface)]
        impl $name {
            #[deprecated]
            #[zbus(property)]
            fn hw_address(&self) -> String {
                self.0.hw_address.clone()
            }

            #[deprecated]
            #[zbus(property)]
            fn carrier(&self) -> bool {
                self.0.carrier
            }

            #[deprecated]
            #[zbus(property)]
            fn slaves(&self) -> Vec<OwnedObjectPath> {
                self.0.slaves.clone()
            }
        }
    };
}

carrier_slaves_interface!(DeviceBond, "org.freedesktop.NetworkManager.Device.Bond");
carrier_slaves_interface!(DeviceBridge, "org.freedesktop.NetworkManager.Device.Bridge");

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceTeam {
    pub base: DeviceCarrierSlaves,
    pub config: String,
}

#[interface(name = "org.freedesktop.NetworkManager.Device.Team")]
impl DeviceTeam {
    #[deprecated]
    #[zbus(property)]
    fn hw_address(&self) -> String {
        self.base.hw_address.clone()
    }

    #[deprecated]
    #[zbus(property)]
    fn carrier(&self) -> bool {
        self.base.carrier
    }

    #[deprecated]
    #[zbus(property)]
    fn slaves(&self) -> Vec<OwnedObjectPath> {
        self.base.slaves.clone()
    }

    #[zbus(property)]
    fn config(&self) -> String {
        self.config.clone()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceVlan {
    pub carrier: bool,
    pub hw_address: String,
    pub parent: OwnedObjectPath,
    pub vlan_id: u32,
}

#[interface(name = "org.freedesktop.NetworkManager.Device.Vlan")]
impl DeviceVlan {
    #[deprecated]
    #[zbus(property)]
    fn hw_address(&self) -> String {
        self.hw_address.clone()
    }

    #[deprecated]
    #[zbus(property)]
    fn carrier(&self) -> bool {
        self.carrier
    }

    #[zbus(property)]
    fn parent(&self) -> OwnedObjectPath {
        self.parent.clone()
    }

    #[zbus(property)]
    fn vlan_id(&self) -> u32 {
        self.vlan_id
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceWireGuard {
    pub fw_mark: u32,
    pub listen_port: u16,
    pub public_key: Vec<u8>,
}

#[interface(name = "org.freedesktop.NetworkManager.Device.WireGuard")]
impl DeviceWireGuard {
    #[zbus(property)]
    fn public_key(&self) -> Vec<u8> {
        self.public_key.clone()
    }

    #[zbus(property)]
    fn listen_port(&self) -> u16 {
        self.listen_port
    }

    #[zbus(property)]
    fn fw_mark(&self) -> u32 {
        self.fw_mark
    }
}

empty_device_interface!(DeviceAdsl, "org.freedesktop.NetworkManager.Device.Adsl");
empty_device_interface!(
    DeviceBluetooth,
    "org.freedesktop.NetworkManager.Device.Bluetooth"
);
empty_device_interface!(DeviceDummy, "org.freedesktop.NetworkManager.Device.Dummy");
empty_device_interface!(
    DeviceGeneric,
    "org.freedesktop.NetworkManager.Device.Generic"
);
empty_device_interface!(DeviceHsr, "org.freedesktop.NetworkManager.Device.Hsr");
empty_device_interface!(
    DeviceIPTunnel,
    "org.freedesktop.NetworkManager.Device.IPTunnel"
);
empty_device_interface!(
    DeviceInfiniband,
    "org.freedesktop.NetworkManager.Device.Infiniband"
);
empty_device_interface!(DeviceIpvlan, "org.freedesktop.NetworkManager.Device.Ipvlan");
empty_device_interface!(DeviceLowpan, "org.freedesktop.NetworkManager.Device.Lowpan");
empty_device_interface!(DeviceMacsec, "org.freedesktop.NetworkManager.Device.Macsec");
empty_device_interface!(
    DeviceMacvlan,
    "org.freedesktop.NetworkManager.Device.Macvlan"
);
empty_device_interface!(DeviceModem, "org.freedesktop.NetworkManager.Device.Modem");
empty_device_interface!(
    DeviceOlpcMesh,
    "org.freedesktop.NetworkManager.Device.OlpcMesh"
);
empty_device_interface!(
    DeviceOvsBridge,
    "org.freedesktop.NetworkManager.Device.OvsBridge"
);
empty_device_interface!(
    DeviceOvsInterface,
    "org.freedesktop.NetworkManager.Device.OvsInterface"
);
empty_device_interface!(
    DeviceOvsPort,
    "org.freedesktop.NetworkManager.Device.OvsPort"
);
empty_device_interface!(DevicePpp, "org.freedesktop.NetworkManager.Device.Ppp");
empty_device_interface!(DeviceTun, "org.freedesktop.NetworkManager.Device.Tun");
empty_device_interface!(DeviceVeth, "org.freedesktop.NetworkManager.Device.Veth");
empty_device_interface!(DeviceVrf, "org.freedesktop.NetworkManager.Device.Vrf");
empty_device_interface!(DeviceVxlan, "org.freedesktop.NetworkManager.Device.Vxlan");
empty_device_interface!(DeviceWpan, "org.freedesktop.NetworkManager.Device.Wpan");

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceWifiP2P {
    pub hw_address: String,
    pub peers: Vec<OwnedObjectPath>,
}

#[interface(name = "org.freedesktop.NetworkManager.Device.WifiP2P")]
impl DeviceWifiP2P {
    #[zbus(signal, name = "PeerAdded")]
    pub(crate) async fn emit_peer_added(
        emitter: &SignalEmitter<'_>,
        peer: OwnedObjectPath,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "PeerRemoved")]
    pub(crate) async fn emit_peer_removed(
        emitter: &SignalEmitter<'_>,
        peer: OwnedObjectPath,
    ) -> zbus::Result<()>;

    fn start_find(
        &self,
        _options: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> fdo::Result<()> {
        Ok(())
    }

    fn stop_find(&self) -> fdo::Result<()> {
        Ok(())
    }

    #[deprecated]
    #[zbus(property)]
    fn hw_address(&self) -> String {
        self.hw_address.clone()
    }

    #[zbus(property)]
    fn peers(&self) -> Vec<OwnedObjectPath> {
        self.peers.clone()
    }
}

pub fn device_aux_interface_names(kind: Kind, type_: Type) -> Vec<&'static str> {
    let mut names = vec!["org.freedesktop.NetworkManager.Device.Statistics"];

    match kind {
        Kind::Bond => names.push("org.freedesktop.NetworkManager.Device.Bond"),
        Kind::Bridge => names.push("org.freedesktop.NetworkManager.Device.Bridge"),
        Kind::Dummy => names.push("org.freedesktop.NetworkManager.Device.Dummy"),
        Kind::Hsr => names.push("org.freedesktop.NetworkManager.Device.Hsr"),
        Kind::Geneve
        | Kind::Gre
        | Kind::Gretap
        | Kind::Ip6gre
        | Kind::Ip6gretap
        | Kind::Ip6tnl
        | Kind::Ipip
        | Kind::Sit => names.push("org.freedesktop.NetworkManager.Device.IPTunnel"),
        Kind::Ipvlan => names.push("org.freedesktop.NetworkManager.Device.Ipvlan"),
        Kind::Lowpan => names.push("org.freedesktop.NetworkManager.Device.Lowpan"),
        Kind::Macsec => names.push("org.freedesktop.NetworkManager.Device.Macsec"),
        Kind::Macvlan => names.push("org.freedesktop.NetworkManager.Device.Macvlan"),
        Kind::Team => names.push("org.freedesktop.NetworkManager.Device.Team"),
        Kind::Tap | Kind::Tun => names.push("org.freedesktop.NetworkManager.Device.Tun"),
        Kind::Veth => names.push("org.freedesktop.NetworkManager.Device.Veth"),
        Kind::Vlan => names.push("org.freedesktop.NetworkManager.Device.Vlan"),
        Kind::Vrf => names.push("org.freedesktop.NetworkManager.Device.Vrf"),
        Kind::Vxlan => names.push("org.freedesktop.NetworkManager.Device.Vxlan"),
        Kind::Wireguard => names.push("org.freedesktop.NetworkManager.Device.WireGuard"),
        _ => {}
    }

    match type_ {
        Type::Bluetooth => names.push("org.freedesktop.NetworkManager.Device.Bluetooth"),
        Type::Infiniband => names.push("org.freedesktop.NetworkManager.Device.Infiniband"),
        Type::Ppp => {
            names.push("org.freedesktop.NetworkManager.Device.Ppp");
            names.push("org.freedesktop.NetworkManager.PPP");
        }
        Type::Wwan => names.push("org.freedesktop.NetworkManager.Device.Modem"),
        Type::Wpan => names.push("org.freedesktop.NetworkManager.Device.Wpan"),
        Type::Unknown | Type::None => {
            if matches!(kind, Kind::Unknown) {
                names.push("org.freedesktop.NetworkManager.Device.Generic");
            }
        }
        _ => {}
    }

    names
}

pub fn maybe_wifi_p2p_interface_name(is_p2p: bool) -> Option<&'static str> {
    if is_p2p {
        Some("org.freedesktop.NetworkManager.Device.WifiP2P")
    } else {
        None
    }
}

pub fn maybe_ppp_interface_names(is_ppp: bool) -> Vec<&'static str> {
    if is_ppp {
        vec![
            "org.freedesktop.NetworkManager.Device.Ppp",
            "org.freedesktop.NetworkManager.PPP",
        ]
    } else {
        Vec::new()
    }
}

#[derive(Clone, Debug)]
pub struct DeviceStatistics {
    interface_name: String,
    refresh_rate_ms: Arc<Mutex<u32>>,
    stats_root: PathBuf,
}

impl DeviceStatistics {
    pub fn new(interface_name: String) -> Self {
        Self {
            interface_name,
            refresh_rate_ms: Arc::new(Mutex::new(0)),
            stats_root: PathBuf::from("/sys/class/net"),
        }
    }

    fn counter(&self, name: &str) -> u64 {
        fs::read_to_string(
            self.stats_root
                .join(&self.interface_name)
                .join("statistics")
                .join(name),
        )
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0)
    }
}

#[interface(name = "org.freedesktop.NetworkManager.Device.Statistics")]
impl DeviceStatistics {
    #[zbus(property)]
    fn refresh_rate_ms(&self) -> u32 {
        *self
            .refresh_rate_ms
            .lock()
            .expect("device statistics mutex poisoned")
    }

    #[zbus(property)]
    fn set_refresh_rate_ms(&self, value: u32) {
        *self
            .refresh_rate_ms
            .lock()
            .expect("device statistics mutex poisoned") = value;
    }

    #[zbus(property)]
    fn tx_bytes(&self) -> u64 {
        self.counter("tx_bytes")
    }

    #[zbus(property)]
    fn rx_bytes(&self) -> u64 {
        self.counter("rx_bytes")
    }
}

pub async fn register_device_aux_interfaces(
    server: &ObjectServer,
    path: &OwnedObjectPath,
    link: &Link,
) -> Result<()> {
    let is_ppp = link.description.name.starts_with("ppp") || link.description.r#type == Type::Ppp;
    server
        .at(
            path.as_str(),
            DeviceStatistics::new(link.description.name.clone()),
        )
        .await?;

    match link.description.kind {
        Kind::Bond => {
            server
                .at(
                    path.as_str(),
                    DeviceBond(DeviceCarrierSlaves {
                        carrier: !matches!(link.carrier_state.as_str(), "off" | "no-carrier"),
                        hw_address: hw_address(link),
                        slaves: synthetic_ports(path),
                    }),
                )
                .await?;
        }
        Kind::Bridge => {
            server
                .at(
                    path.as_str(),
                    DeviceBridge(DeviceCarrierSlaves {
                        carrier: !matches!(link.carrier_state.as_str(), "off" | "no-carrier"),
                        hw_address: hw_address(link),
                        slaves: synthetic_ports(path),
                    }),
                )
                .await?;
        }
        Kind::Dummy => {
            server.at(path.as_str(), DeviceDummy).await?;
        }
        Kind::Hsr => {
            server.at(path.as_str(), DeviceHsr).await?;
        }
        Kind::Geneve
        | Kind::Gre
        | Kind::Gretap
        | Kind::Ip6gre
        | Kind::Ip6gretap
        | Kind::Ip6tnl
        | Kind::Ipip
        | Kind::Sit => {
            server.at(path.as_str(), DeviceIPTunnel).await?;
        }
        Kind::Ipvlan => {
            server.at(path.as_str(), DeviceIpvlan).await?;
        }
        Kind::Lowpan => {
            server.at(path.as_str(), DeviceLowpan).await?;
        }
        Kind::Macsec => {
            server.at(path.as_str(), DeviceMacsec).await?;
        }
        Kind::Macvlan => {
            server.at(path.as_str(), DeviceMacvlan).await?;
        }
        Kind::Team => {
            server
                .at(
                    path.as_str(),
                    DeviceTeam {
                        base: DeviceCarrierSlaves {
                            carrier: !matches!(link.carrier_state.as_str(), "off" | "no-carrier"),
                            hw_address: hw_address(link),
                            slaves: synthetic_ports(path),
                        },
                        config: String::from("{}"),
                    },
                )
                .await?;
        }
        Kind::Tap | Kind::Tun => {
            server.at(path.as_str(), DeviceTun).await?;
        }
        Kind::Veth => {
            server.at(path.as_str(), DeviceVeth).await?;
        }
        Kind::Vlan => {
            server
                .at(
                    path.as_str(),
                    DeviceVlan {
                        carrier: !matches!(link.carrier_state.as_str(), "off" | "no-carrier"),
                        hw_address: hw_address(link),
                        parent: OwnedObjectPath::default(),
                        vlan_id: 0,
                    },
                )
                .await?;
        }
        Kind::Vrf => {
            server.at(path.as_str(), DeviceVrf).await?;
        }
        Kind::Vxlan => {
            server.at(path.as_str(), DeviceVxlan).await?;
        }
        Kind::Wireguard => {
            server
                .at(
                    path.as_str(),
                    DeviceWireGuard {
                        fw_mark: 0,
                        listen_port: 0,
                        public_key: Vec::new(),
                    },
                )
                .await?;
        }
        _ => {}
    }

    match link.description.r#type {
        Type::Bluetooth => {
            server.at(path.as_str(), DeviceBluetooth).await?;
        }
        Type::Infiniband => {
            server.at(path.as_str(), DeviceInfiniband).await?;
        }
        Type::Wlan if link.description.wireless_lan_interface_type == "p2p-device" => {
            server
                .at(
                    path.as_str(),
                    DeviceWifiP2P {
                        hw_address: link
                            .description
                            .hardware_address
                            .iter()
                            .map(|byte| format!("{byte:02x}"))
                            .collect::<Vec<_>>()
                            .join(":"),
                        peers: Vec::new(),
                    },
                )
                .await?;
        }
        Type::Wwan => {
            server.at(path.as_str(), DeviceModem).await?;
        }
        Type::Wpan => {
            server.at(path.as_str(), DeviceWpan).await?;
        }
        Type::Unknown | Type::None => {
            if matches!(link.description.kind, Kind::Unknown) {
                server.at(path.as_str(), DeviceGeneric).await?;
            }
        }
        _ => {}
    }

    if is_ppp {
        server.at(path.as_str(), DevicePpp).await?;
        server
            .at(path.as_str(), crate::network_manager::ppp::Ppp)
            .await?;
    }

    Ok(())
}

pub async fn unregister_device_aux_interfaces(
    server: &ObjectServer,
    path: &OwnedObjectPath,
    kind: Kind,
    type_: Type,
) {
    let _ = server.remove::<DeviceStatistics, _>(path.as_str()).await;

    match kind {
        Kind::Bond => {
            let _ = server.remove::<DeviceBond, _>(path.as_str()).await;
        }
        Kind::Bridge => {
            let _ = server.remove::<DeviceBridge, _>(path.as_str()).await;
        }
        Kind::Dummy => {
            let _ = server.remove::<DeviceDummy, _>(path.as_str()).await;
        }
        Kind::Hsr => {
            let _ = server.remove::<DeviceHsr, _>(path.as_str()).await;
        }
        Kind::Geneve
        | Kind::Gre
        | Kind::Gretap
        | Kind::Ip6gre
        | Kind::Ip6gretap
        | Kind::Ip6tnl
        | Kind::Ipip
        | Kind::Sit => {
            let _ = server.remove::<DeviceIPTunnel, _>(path.as_str()).await;
        }
        Kind::Ipvlan => {
            let _ = server.remove::<DeviceIpvlan, _>(path.as_str()).await;
        }
        Kind::Lowpan => {
            let _ = server.remove::<DeviceLowpan, _>(path.as_str()).await;
        }
        Kind::Macsec => {
            let _ = server.remove::<DeviceMacsec, _>(path.as_str()).await;
        }
        Kind::Macvlan => {
            let _ = server.remove::<DeviceMacvlan, _>(path.as_str()).await;
        }
        Kind::Team => {
            let _ = server.remove::<DeviceTeam, _>(path.as_str()).await;
        }
        Kind::Tap | Kind::Tun => {
            let _ = server.remove::<DeviceTun, _>(path.as_str()).await;
        }
        Kind::Veth => {
            let _ = server.remove::<DeviceVeth, _>(path.as_str()).await;
        }
        Kind::Vlan => {
            let _ = server.remove::<DeviceVlan, _>(path.as_str()).await;
        }
        Kind::Vrf => {
            let _ = server.remove::<DeviceVrf, _>(path.as_str()).await;
        }
        Kind::Vxlan => {
            let _ = server.remove::<DeviceVxlan, _>(path.as_str()).await;
        }
        Kind::Wireguard => {
            let _ = server.remove::<DeviceWireGuard, _>(path.as_str()).await;
        }
        _ => {}
    }

    match type_ {
        Type::Bluetooth => {
            let _ = server.remove::<DeviceBluetooth, _>(path.as_str()).await;
        }
        Type::Infiniband => {
            let _ = server.remove::<DeviceInfiniband, _>(path.as_str()).await;
        }
        Type::Wlan => {
            let _ = server.remove::<DeviceWifiP2P, _>(path.as_str()).await;
        }
        Type::Wwan => {
            let _ = server.remove::<DeviceModem, _>(path.as_str()).await;
        }
        Type::Wpan => {
            let _ = server.remove::<DeviceWpan, _>(path.as_str()).await;
        }
        Type::Unknown | Type::None => {
            if matches!(kind, Kind::Unknown) {
                let _ = server.remove::<DeviceGeneric, _>(path.as_str()).await;
            }
        }
        _ => {}
    }

    if path
        .as_str()
        .rsplit('/')
        .next()
        .is_some_and(|segment| segment.starts_with("ppp"))
    {
        let _ = server.remove::<DevicePpp, _>(path.as_str()).await;
        let _ = server
            .remove::<crate::network_manager::ppp::Ppp, _>(path.as_str())
            .await;
    }
}
