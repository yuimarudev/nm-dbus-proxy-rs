// Modified by yuimarudev on 2026-03-23.
// This file contains changes from the original upstream work.
use serde::Deserialize;

/// [Link Object]( https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.network1.html#Link%20Object )
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Link {
    pub address_state: String,
    pub administrative_state: String,
    pub bit_rates: (u64, u64),
    pub carrier_state: String,
    pub description: LinkDescription,
    pub ipv4_address_state: String,
    pub ipv6_address_state: String,
    pub online_state: String,
    pub operational_state: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct LinkDescription {
    #[serde(default)]
    pub addresses: Vec<Address>,
    #[serde(default)]
    pub alternative_names: Vec<String>,
    #[serde(default)]
    pub driver: String,
    #[serde(default, rename = "HardwareAddress")]
    pub hardware_address: Vec<u8>,
    pub index: usize,
    #[serde(default)]
    pub kind: Kind,
    #[serde(rename = "MTU")]
    pub mtu: u32,
    pub name: String,
    #[serde(default, rename = "PermanentHardwareAddress")]
    pub permanent_hardware_address: Vec<u8>,
    pub r#type: Type,
    #[serde(default, rename = "WirelessLanInterfaceTypeString")]
    pub wireless_lan_interface_type: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Address {
    #[serde(default)]
    pub address_string: String,
    #[serde(default)]
    pub config_state: String,
    #[serde(default)]
    pub family: u8,
    #[serde(default)]
    pub prefix_length: u8,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bareudp,
    Bond,
    Bridge,
    Dummy,
    Geneve,
    Gre,
    Gretap,
    Hsr,
    Ip6gre,
    Ip6gretap,
    Ip6tnl,
    Ipip,
    Ipvlan,
    Lowpan,
    Macsec,
    Macvlan,
    Sit,
    Tap,
    Team,
    Tun,
    #[default]
    Unknown,
    Veth,
    Vlan,
    Vrf,
    Vxlan,
    Wireguard,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    Bluetooth,
    Ether,
    Infiniband,
    Loopback,
    None,
    Ppp,
    #[default]
    Unknown,
    Wlan,
    Wpan,
    Wwan,
}
