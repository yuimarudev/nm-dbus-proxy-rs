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
    pub driver: String,
    pub index: usize,
    #[serde(default)]
    pub kind: Kind,
    #[serde(rename = "MTU")]
    pub mtu: u32,
    pub name: String,
    pub r#type: Type,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bond,
    Bridge,
    Gre,
    Tun,
    #[default]
    Unknown,
    Veth,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    Ether,
    Loopback,
    None,
    #[default]
    Unknown,
    Wlan,
    Wwan,
}
