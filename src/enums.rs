/// see: [NMActivationStateFlags](https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMActivationStateFlags)
pub enum NMActivationStateFlags {
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

/// see: [NMDeviceState](https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMDeviceState)
pub enum NMDeviceState {
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

/// see: [NMDeviceType](https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMDeviceType)
pub enum NMDeviceType {
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
