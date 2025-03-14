use zbus::{
    interface,
    zvariant::{ObjectPath, OwnedObjectPath},
};

use crate::enums::{NMActivationStateFlags, NMActiveConnectionState};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ActiveConnection {
    pub devices: Vec<OwnedObjectPath>,
    pub id: String,
}

/// [NetworkManager.Connection.Active]( https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Connection.Active.html )
#[interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl ActiveConnection {
    // /// StateChanged signal
    // #[zbus(signal)]
    // async fn state_changed(&self, _state: u32, _reason: u32) -> zbus::Result<()>;

    /// Connection property
    #[zbus(property)]
    fn connection(&self) -> OwnedObjectPath {
        // TODO
        OwnedObjectPath::from(
            ObjectPath::try_from("/org/freedesktop/NetworkManager/Settings/TODO")
                .expect("should parse object path"),
        )
    }

    /// Controller property
    #[zbus(property)]
    fn controller(&self) -> OwnedObjectPath {
        // TODO ??
        OwnedObjectPath::default()
    }

    /// Default property
    #[zbus(property)]
    fn default(&self) -> bool {
        // TODO
        false
    }

    /// Default6 property
    #[zbus(property)]
    fn default6(&self) -> bool {
        // TODO
        false
    }

    /// Devices property
    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        self.devices.clone()
    }

    /// Dhcp4Config property
    #[zbus(property)]
    fn dhcp4_config(&self) -> OwnedObjectPath {
        // TODO
        OwnedObjectPath::from(
            ObjectPath::try_from("/org/freedesktop/NetworkManager/DHCP4Config/TODO")
                .expect("should parse object path"),
        )
    }

    /// Dhcp6Config property
    #[zbus(property)]
    fn dhcp6_config(&self) -> OwnedObjectPath {
        // TODO
        OwnedObjectPath::from(
            ObjectPath::try_from("/org/freedesktop/NetworkManager/DHCP6Config/TODO")
                .expect("should parse object path"),
        )
    }

    /// Id property
    #[zbus(property)]
    fn id(&self) -> String {
        self.id.clone()
    }

    /// Ip4Config property
    #[zbus(property)]
    fn ip4_config(&self) -> OwnedObjectPath {
        // TODO
        OwnedObjectPath::from(
            ObjectPath::try_from("/org/freedesktop/NetworkManager/IP4Config/TODO")
                .expect("should parse object path"),
        )
    }

    /// Ip6Config property
    #[zbus(property)]
    fn ip6_config(&self) -> OwnedObjectPath {
        // TODO
        OwnedObjectPath::from(
            ObjectPath::try_from("/org/freedesktop/NetworkManager/IP6Config/TODO")
                .expect("should parse object path"),
        )
    }

    /// SpecificObject property
    #[zbus(property)]
    fn specific_object(&self) -> OwnedObjectPath {
        // TODO ?? maybe this is org.freedesktop.NetworkManager.VPN.Connection if any?
        OwnedObjectPath::default()
    }

    /// State property
    #[zbus(property)]
    fn state(&self) -> u32 {
        // TODO
        NMActiveConnectionState::Activated as u32
    }

    /// StateFlags property
    #[zbus(property)]
    fn state_flags(&self) -> u32 {
        // TODO
        NMActivationStateFlags::None as u32
    }

    /// Type property
    #[zbus(property)]
    fn type_(&self) -> String {
        // TODO
        String::from("ethernet")
    }

    /// Uuid property
    #[zbus(property)]
    fn uuid(&self) -> String {
        // TODO
        String::from("TODO")
    }

    /// Vpn property
    #[zbus(property)]
    fn vpn(&self) -> bool {
        // TODO
        false
    }
}
