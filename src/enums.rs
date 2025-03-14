use tracing::warn;

use crate::systemd_networkd::link::{Kind, Type};

/// [NM80211Mode]( https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NM80211Mode )
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NM80211Mode {
    #[default]
    Unknown = 0,
    Adhoc = 1,
    Infra = 2,
    Ap = 3,
    Mesh = 4,
}

/// see: [NMActivationStateFlags](https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMActivationStateFlags)
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NMActivationStateFlags {
    #[default]
    None = 0x0,
    IsController = 0x1,
    IsPort = 0x2,
    Layer2Ready = 0x4,
    Ip4Ready = 0x8,
    Ip6Ready = 0x10,
    ControllerHasPorts = 0x20,
    LifetimeBoundToProfileVisibility = 0x40,
    External = 0x80,
}

/// see: [NMActiveConnectionState](https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMActiveConnectionState)
pub enum NMActiveConnectionState {
    Unknown = 0,
    Activating = 1,
    Activated = 2,
    Deactivating = 3,
    Deactivated = 4,
}

/// see: [NMConnectivityState](https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMConnectivityState)
pub enum NMConnectivityState {
    Unknown = 0,
    None = 1,
    Portal = 2,
    Limited = 3,
    Full = 4,
}

/// [NMDeviceInterfaceFlags]( https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMDeviceInterfaceFlags )
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NMDeviceInterfaceFlags {
    #[default]
    None = 0,
    Up = 0x1,
    LowerUp = 0x2,
    Promisc = 0x4,
    Carrier = 0x10000,
    LldpClientEnabled = 0x20000,
}

/// see: [NMDeviceState]( https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMDeviceState )
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NMDeviceState {
    #[default]
    Unknown = 0,
    Unmanaged = 10,
    Unavailable = 20,
    Disconnected = 30,
    Prepare = 40,
    Config = 50,
    NeedAuth = 60,
    IpConfig = 70,
    IpCheck = 80,
    Secondaries = 90,
    Activated = 100,
    Deactivating = 110,
    Failed = 120,
}

/// [NMDeviceStateReason]( https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMDeviceStateReason )
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NMDeviceStateReason {
    None = 0,
    #[default]
    Unknown = 1,
    NowManaged = 2,
    NowUnmanaged = 3,
    ConfigFailed = 4,
    IpConfigUnavailable = 5,
    IpConfigExpired = 6,
    NoSecrets = 7,
    SupplicantDisconnect = 8,
    SupplicantConfigFailed = 9,
    SupplicantFailed = 10,
    SupplicantTimeout = 11,
    PppStartFailed = 12,
    PppDisconnect = 13,
    PppFailed = 14,
    DhcpStartFailed = 15,
    DhcpError = 16,
    DhcpFailed = 17,
    SharedStartFailed = 18,
    SharedFailed = 19,
    AutoipStartFailed = 20,
    AutoipError = 21,
    AutoipFailed = 22,
    ModemBusy = 23,
    ModemNoDialTone = 24,
    ModemNoCarrier = 25,
    ModemDialTimeout = 26,
    ModemDialFailed = 27,
    ModemInitFailed = 28,
    GsmApnFailed = 29,
    GsmRegistrationNotSearching = 30,
    GsmRegistrationDenied = 31,
    GsmRegistrationTimeout = 32,
    GsmRegistrationFailed = 33,
    GsmPinCheckFailed = 34,
    FirmwareMissing = 35,
    Removed = 36,
    Sleeping = 37,
    ConnectionRemoved = 38,
    UserRequested = 39,
    Carrier = 40,
    ConnectionAssumed = 41,
    SupplicantAvailable = 42,
    ModemNotFound = 43,
    BtFailed = 44,
    GsmSimNotInserted = 45,
    GsmSimPinRequired = 46,
    GsmSimPukRequired = 47,
    GsmSimWrong = 48,
    InfinibandMode = 49,
    DependencyFailed = 50,
    Br2684Failed = 51,
    ModemManagerUnavailable = 52,
    SsidNotFound = 53,
    SecondaryConnectionFailed = 54,
    DcbFcoeFailed = 55,
    TeamdControlFailed = 56,
    ModemFailed = 57,
    ModemAvailable = 58,
    SimPinIncorrect = 59,
    NewActivation = 60,
    ParentChanged = 61,
    ParentManagedChanged = 62,
    OvsdbFailed = 63,
    IpAddressDuplicate = 64,
    IpMethodUnsupported = 65,
    SriovConfigurationFailed = 66,
    PeerNotFound = 67,
    DeviceHandlerFailed = 68,
    UnmanagedByDefault = 69,
    UnmanagedExternalDown = 70,
    UnmanagedLinkNotInit = 71,
    UnmanagedQuitting = 72,
    UnmanagedSleeping = 73,
    UnmanagedUserConf = 74,
    UnmanagedUserExplicit = 75,
    UnmanagedUserSettings = 76,
    UnmanagedUserUdev = 77,
}

/// see: [NMDeviceType]( https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMDeviceType )
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NMDeviceType {
    #[default]
    Unknown = 0,
    Ethernet = 1,
    Wifi = 2,
    Unused1 = 3,
    Unused2 = 4,
    Bluetooth = 5,
    OlpcMesh = 6,
    WiMax = 7,
    Modem = 8,
    Infiniband = 9,
    Bond = 10,
    Vlan = 11,
    Adsl = 12,
    Bridge = 13,
    Generic = 14,
    Team = 15,
    Tun = 16,
    IpTunnel = 17,
    MacVlan = 18,
    VxLan = 19,
    VEth = 20,
    MacSec = 21,
    Dummy = 22,
    Ppp = 23,
    OvsInterface = 24,
    OvsPort = 25,
    OvsBridge = 26,
    WPan = 27,
    SixLowPan = 28,
    WireGuard = 29,
    WifiP2P = 30,
    Vrf = 31,
    Loopback = 32,
    Hsr = 33,
}

impl From<(Kind, Type)> for NMDeviceType {
    fn from(value: (Kind, Type)) -> Self {
        match value {
            (Kind::Bond, _) => Self::Bond,
            (Kind::Bridge, _) => Self::Bridge,
            (Kind::Tun, _) => Self::Tun,
            (Kind::Veth, _) => Self::VEth,
            (_, Type::Ether) => Self::Ethernet,
            (_, Type::Loopback) => Self::Loopback,
            (_, Type::Wlan) => Self::Wifi,
            _ => {
                warn!(value = ?value, "unknown device type");
                Self::Unknown
            }
        }
    }
}

/// [NMMetered]( https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMMetered )
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NMMetered {
    #[default]
    Unknown = 0,
    Yes = 1,
    No = 2,
    GuessYes = 3,
    GuessNo = 4,
}

/// see: [NMRadioFlags]( https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMRadioFlags )
pub enum NMRadioFlags {
    None = 0,
    WlanAvailable = 1,
    WwanAvailable = 2,
}
