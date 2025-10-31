use std::collections::HashMap;

use zbus::{
    interface,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue},
};

pub mod loopback;
pub mod wired;
pub mod wireless;

use crate::enums::{
    NMConnectivityState, NMDeviceInterfaceFlags, NMDeviceState, NMDeviceStateReason, NMDeviceType,
    NMMetered,
};

/// see: [Device]( https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.html )
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Device {
    pub active_connection: OwnedObjectPath,
    pub driver: String,
    pub interface: String,
    pub ip_interface: String,
    pub mtu: u32,
    pub path: String,
    pub state: NMDeviceState,
    pub state_reason: (NMDeviceState, NMDeviceStateReason),
    pub r#type: NMDeviceType,
    pub udi: String,
}

#[interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    // /// Delete method
    // fn delete(&self) -> zbus::Result<()>;

    // /// Disconnect method
    // fn disconnect(&self) -> zbus::Result<()>;

    // /// GetAppliedConnection method
    // fn get_applied_connection(
    //     &self,
    //     flags: u32,
    // ) -> zbus::Result<(
    //     std::collections::HashMap<
    //         String,
    //         std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    //     >,
    //     u64,
    // )>;

    // /// Reapply method
    // fn reapply(
    //     &self,
    //     connection: std::collections::HashMap<
    //         &str,
    //         std::collections::HashMap<&str, &zbus::zvariant::Value<'_>>,
    //     >,
    //     version_id: u64,
    //     flags: u32,
    // ) -> zbus::Result<()>;

    // /// StateChanged signal
    // #[zbus(signal)]
    // fn state_changed(&self, new_state: u32, old_state: u32, reason: u32) -> zbus::Result<()>;

    /// `ActiveConnection` property
    #[zbus(property)]
    fn active_connection(&self) -> OwnedObjectPath {
        self.active_connection.clone()
    }

    /// Autoconnect property
    #[zbus(property)]
    fn autoconnect(&self) -> bool {
        // TODO
        true
    }
    // #[zbus(property)]
    // fn set_autoconnect(&self, value: bool) -> () {
    //     todo!()
    // }

    /// `AvailableConnections` property
    #[zbus(property)]
    fn available_connections(&self) -> Vec<OwnedObjectPath> {
        // TODO
        vec![self.active_connection.clone()]
    }

    /// Capabilities property
    #[zbus(property)]
    fn capabilities(&self) -> u32 {
        // TODO
        0
    }

    /// `DeviceType` property
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        self.r#type as u32
    }

    /// `Dhcp4Config` property
    #[zbus(property)]
    fn dhcp4_config(&self) -> OwnedObjectPath {
        // TODO
        OwnedObjectPath::default()
    }

    /// `Dhcp6Config` property
    #[zbus(property)]
    fn dhcp6_config(&self) -> OwnedObjectPath {
        // TODO
        OwnedObjectPath::default()
    }

    /// Driver property
    #[zbus(property)]
    fn driver(&self) -> String {
        self.driver.clone()
    }

    /// `DriverVersion` property
    #[zbus(property)]
    fn driver_version(&self) -> String {
        // TODO
        String::new()
    }

    /// `FirmwareMissing` property
    #[zbus(property)]
    fn firmware_missing(&self) -> bool {
        // TODO
        false
    }

    /// `FirmwareVersion` property
    #[zbus(property)]
    fn firmware_version(&self) -> String {
        // TODO
        String::new()
    }

    /// `HwAddress` property
    #[zbus(property)]
    fn hw_address(&self) -> String {
        // TODO
        String::new()
    }

    /// Interface property
    #[zbus(property)]
    fn interface(&self) -> String {
        self.interface.clone()
    }

    /// `InterfaceFlags` property
    #[zbus(property)]
    fn interface_flags(&self) -> u32 {
        // TODO
        NMDeviceInterfaceFlags::default() as u32
    }

    /// `Ip4Config` property
    #[zbus(property)]
    fn ip4_config(&self) -> OwnedObjectPath {
        // TODO
        OwnedObjectPath::from(
            ObjectPath::try_from("/org/freedesktop/NetworkManager/IP4Config/1")
                .expect("should parse device object path"),
        )
    }

    /// `Ip4Connectivity` property
    #[zbus(property)]
    fn ip4_connectivity(&self) -> u32 {
        // TODO
        NMConnectivityState::Full as u32
    }

    /// `Ip6Config` property
    #[zbus(property)]
    fn ip6_config(&self) -> OwnedObjectPath {
        // TODO
        OwnedObjectPath::default()
    }

    /// `Ip6Connectivity` property
    #[zbus(property)]
    fn ip6_connectivity(&self) -> u32 {
        // TODO
        NMConnectivityState::Full as u32
    }

    /// `IpInterface` property
    #[zbus(property)]
    fn ip_interface(&self) -> String {
        self.ip_interface.clone()
    }

    /// `LldpNeighbors` property
    #[zbus(property)]
    fn lldp_neighbors(&self) -> Vec<HashMap<String, OwnedValue>> {
        // TODO
        vec![]
    }

    /// Managed property
    #[zbus(property)]
    fn managed(&self) -> bool {
        // TODO
        true
    }
    // #[zbus(property)]
    // fn set_managed(&self, value: bool) -> () {
    //     todo!()
    // }

    /// Metered property
    #[zbus(property)]
    fn metered(&self) -> u32 {
        // TODO
        NMMetered::default() as u32
    }

    /// Mtu property
    #[zbus(property)]
    fn mtu(&self) -> u32 {
        self.mtu
    }

    /// `NmPluginMissing` property
    #[zbus(property)]
    fn nm_plugin_missing(&self) -> bool {
        // TODO
        false
    }

    /// Path property
    #[zbus(property)]
    fn path(&self) -> String {
        self.path.clone()
    }

    /// `PhysicalPortId` property
    #[zbus(property)]
    fn physical_port_id(&self) -> String {
        // TODO
        String::new()
    }

    /// Ports property
    #[zbus(property)]
    fn ports(&self) -> Vec<OwnedObjectPath> {
        // TODO
        vec![]
    }

    /// Real property
    #[zbus(property)]
    fn real(&self) -> bool {
        // TODO
        true
    }

    /// State property
    #[zbus(property)]
    fn state(&self) -> u32 {
        // TODO
        NMDeviceState::Activated as u32
    }

    /// `StateReason` property
    #[zbus(property)]
    fn state_reason(&self) -> (u32, u32) {
        // TODO
        (self.state_reason.0 as u32, self.state_reason.1 as u32)
    }

    /// Udi property
    #[zbus(property)]
    fn udi(&self) -> String {
        self.udi.clone()
    }
}
