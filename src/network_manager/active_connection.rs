// Modified by yuimarudev on 2026-03-23.
// This file contains changes from the original upstream work.
use zbus::{interface, object_server::SignalEmitter, zvariant::OwnedObjectPath};

use crate::enums::NMActiveConnectionState;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ActiveConnection {
    pub connection: OwnedObjectPath,
    pub controller: OwnedObjectPath,
    pub default: bool,
    pub default6: bool,
    pub devices: Vec<OwnedObjectPath>,
    pub dhcp4_config: OwnedObjectPath,
    pub dhcp6_config: OwnedObjectPath,
    pub id: String,
    pub ip4_config: OwnedObjectPath,
    pub ip6_config: OwnedObjectPath,
    pub specific_object: OwnedObjectPath,
    pub state: NMActiveConnectionState,
    pub state_flags: u32,
    pub type_: String,
    pub uuid: String,
    pub vpn: bool,
}

/// [NetworkManager.Connection.Active]( https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Connection.Active.html )
#[interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl ActiveConnection {
    #[zbus(signal, name = "StateChanged")]
    pub(crate) async fn emit_state_changed(
        emitter: &SignalEmitter<'_>,
        state: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    /// Connection property
    #[zbus(property)]
    fn connection(&self) -> OwnedObjectPath {
        self.connection.clone()
    }

    /// Controller property
    #[zbus(property)]
    fn controller(&self) -> OwnedObjectPath {
        self.controller.clone()
    }

    /// Default property
    #[zbus(property)]
    fn default(&self) -> bool {
        self.default
    }

    /// Default6 property
    #[zbus(property)]
    fn default6(&self) -> bool {
        self.default6
    }

    /// Devices property
    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        self.devices.clone()
    }

    /// `Dhcp4Config` property
    #[zbus(property)]
    fn dhcp4_config(&self) -> OwnedObjectPath {
        self.dhcp4_config.clone()
    }

    /// `Dhcp6Config` property
    #[zbus(property)]
    fn dhcp6_config(&self) -> OwnedObjectPath {
        self.dhcp6_config.clone()
    }

    /// Id property
    #[zbus(property)]
    fn id(&self) -> String {
        self.id.clone()
    }

    /// `Ip4Config` property
    #[zbus(property)]
    fn ip4_config(&self) -> OwnedObjectPath {
        self.ip4_config.clone()
    }

    /// `Ip6Config` property
    #[zbus(property)]
    fn ip6_config(&self) -> OwnedObjectPath {
        self.ip6_config.clone()
    }

    /// `SpecificObject` property
    #[zbus(property)]
    fn specific_object(&self) -> OwnedObjectPath {
        self.specific_object.clone()
    }

    /// State property
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.state as u32
    }

    /// `StateFlags` property
    #[zbus(property)]
    fn state_flags(&self) -> u32 {
        self.state_flags
    }

    /// Type property
    #[zbus(property)]
    fn type_(&self) -> String {
        self.type_.clone()
    }

    /// Uuid property
    #[zbus(property)]
    fn uuid(&self) -> String {
        self.uuid.clone()
    }

    /// Vpn property
    #[zbus(property)]
    fn vpn(&self) -> bool {
        self.vpn
    }
}
