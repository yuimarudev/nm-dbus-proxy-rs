use zbus::{
    interface,
    zvariant::{ObjectPath, OwnedObjectPath},
};

use crate::enums::NMConnectivityState;

/// see: [NetworkManager](https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.html)
pub struct NetworkManager;

#[interface(name = "org.freedesktop.NetworkManager")]
impl NetworkManager {
    // /// ActivateConnection method
    // fn activate_connection(
    //     &self,
    //     connection: &zbus::zvariant::ObjectPath<'_>,
    //     device: &zbus::zvariant::ObjectPath<'_>,
    //     specific_object: &zbus::zvariant::ObjectPath<'_>,
    // ) -> zbus::Result<zbus::zvariant::OwnedObjectPath> {
    //     todo!()
    // }

    // /// AddAndActivateConnection method
    // fn add_and_activate_connection(
    //     &self,
    //     connection: std::collections::HashMap<
    //         &str,
    //         std::collections::HashMap<&str, &zbus::zvariant::Value<'_>>,
    //     >,
    //     device: &zbus::zvariant::ObjectPath<'_>,
    //     specific_object: &zbus::zvariant::ObjectPath<'_>,
    // ) -> zbus::Result<(
    //     zbus::zvariant::OwnedObjectPath,
    //     zbus::zvariant::OwnedObjectPath,
    // )> {
    //     todo!()
    // }

    // /// AddAndActivateConnection2 method
    // #[allow(clippy::too_many_arguments)]
    // fn add_and_activate_connection2(
    //     &self,
    //     connection: std::collections::HashMap<
    //         &str,
    //         std::collections::HashMap<&str, &zbus::zvariant::Value<'_>>,
    //     >,
    //     device: &zbus::zvariant::ObjectPath<'_>,
    //     specific_object: &zbus::zvariant::ObjectPath<'_>,
    //     options: std::collections::HashMap<&str, &zbus::zvariant::Value<'_>>,
    // ) -> zbus::Result<(
    //     zbus::zvariant::OwnedObjectPath,
    //     zbus::zvariant::OwnedObjectPath,
    //     std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    // )> {
    //     todo!()
    // }

    /// CheckConnectivity method
    fn check_connectivity(&self) -> u32 {
        NMConnectivityState::Full as u32
    }

    // /// CheckpointAdjustRollbackTimeout method
    // fn checkpoint_adjust_rollback_timeout(
    //     &self,
    //     checkpoint: &zbus::zvariant::ObjectPath<'_>,
    //     add_timeout: u32,
    // ) -> zbus::Result<()> {
    //     todo!()
    // }

    // /// CheckpointCreate method
    // fn checkpoint_create(
    //     &self,
    //     devices: &[&zbus::zvariant::ObjectPath<'_>],
    //     rollback_timeout: u32,
    //     flags: u32,
    // ) -> zbus::Result<zbus::zvariant::OwnedObjectPath> {
    //     todo!()
    // }

    // /// CheckpointDestroy method
    // fn checkpoint_destroy(&self, checkpoint: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()> {
    //     todo!()
    // }

    // /// CheckpointRollback method
    // fn checkpoint_rollback(
    //     &self,
    //     checkpoint: &zbus::zvariant::ObjectPath<'_>,
    // ) -> zbus::Result<std::collections::HashMap<String, u32>> {
    //     todo!()
    // }

    // /// DeactivateConnection method
    // fn deactivate_connection(
    //     &self,
    //     active_connection: &zbus::zvariant::ObjectPath<'_>,
    // ) -> zbus::Result<()> {
    //     todo!()
    // }

    // /// Enable method
    // fn enable(&self, enable: bool) -> zbus::Result<()> {
    //     todo!()
    // }

    // /// GetAllDevices method
    // fn get_all_devices(&self) -> zbus::Result<Vec<OwnedObjectPath>> {
    //     todo!()
    // }

    // /// GetDeviceByIpIface method
    // fn get_device_by_ip_iface(&self, iface: &str) -> zbus::Result<OwnedObjectPath> {
    //     todo!()
    // }

    // /// GetDevices method
    // fn get_devices(&self) -> zbus::Result<Vec<OwnedObjectPath>> {
    //     todo!()
    // }

    // /// GetLogging method
    // fn get_logging(&self) -> zbus::Result<(String, String)> {
    //     todo!()
    // }

    // /// GetPermissions method
    // fn get_permissions(&self) -> zbus::Result<std::collections::HashMap<String, String>> {
    //     todo!()
    // }

    // /// Reload method
    // fn reload(&self, flags: u32) -> zbus::Result<()> {
    //     todo!()
    // }

    // /// SetLogging method
    // fn set_logging(&self, level: &str, domains: &str) -> zbus::Result<()> {
    //     todo!()
    // }

    // /// Sleep method
    // fn sleep(&self, sleep: bool) -> zbus::Result<()> {
    //     todo!()
    // }

    // /// state method
    // #[zbus(name = "state")]
    // fn state(&self) -> zbus::Result<u32> {
    //     todo!()
    // }

    // /// CheckPermissions signal
    // #[zbus(signal)]
    // async fn check_permissions(signal_emitter: &SignalEmitter<'_>) -> ();

    // /// DeviceAdded signal
    // #[zbus(signal)]
    // async fn device_added(
    //     signal_emitter: &SignalEmitter<'_>,
    //     device_path: zbus::zvariant::ObjectPath<'_>,
    // ) -> ();

    // /// DeviceRemoved signal
    // #[zbus(signal)]
    // async fn device_removed(
    //     signal_emitter: &SignalEmitter<'_>,
    //     device_path: zbus::zvariant::ObjectPath<'_>,
    // ) -> ();

    // /// StateChanged signal
    // #[zbus(signal)]
    // async fn state_changed(signal_emitter: &SignalEmitter<'_>, state: u32) -> zbus::Result<()>;

    /// ActivatingConnection property
    #[zbus(property)]
    fn activating_connection(&self) -> OwnedObjectPath {
        todo!()
    }

    /// ActiveConnections property
    #[zbus(property)]
    fn active_connections(&self) -> Vec<OwnedObjectPath> {
        vec![
            ObjectPath::try_from("/org/freedesktop/NetworkManager/ActiveConnection/1")
                .expect("should parse into D-Bus object path")
                .into(),
        ]
    }

    /// AllDevices property
    #[zbus(property)]
    fn all_devices(&self) -> Vec<OwnedObjectPath> {
        todo!()
    }

    /// Capabilities property
    #[zbus(property)]
    fn capabilities(&self) -> Vec<u32> {
        todo!()
    }

    /// Checkpoints property
    #[zbus(property)]
    fn checkpoints(&self) -> Vec<OwnedObjectPath> {
        todo!()
    }

    /// Connectivity property
    #[zbus(property)]
    fn connectivity(&self) -> u32 {
        NMConnectivityState::Full as u32
    }

    /// ConnectivityCheckAvailable property
    #[zbus(property)]
    fn connectivity_check_available(&self) -> bool {
        todo!()
    }

    /// ConnectivityCheckEnabled property
    #[zbus(property)]
    fn connectivity_check_enabled(&self) -> bool {
        todo!()
    }
    // #[zbus(property)]
    // fn set_connectivity_check_enabled(&self, value: bool) -> zbus::Result<()> {
    //     todo!()
    // }

    /// ConnectivityCheckUri property
    #[zbus(property)]
    fn connectivity_check_uri(&self) -> String {
        todo!()
    }

    /// Devices property
    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        todo!()
    }

    /// GlobalDnsConfiguration property
    #[zbus(property)]
    fn global_dns_configuration(
        &self,
    ) -> std::collections::HashMap<String, zbus::zvariant::OwnedValue> {
        todo!()
    }
    // #[zbus(property)]
    // fn set_global_dns_configuration(
    //     &self,
    //     value: std::collections::HashMap<&str, &zbus::zvariant::Value<'_>>,
    // ) -> zbus::Result<()> {
    //     todo!()
    // }

    /// Metered property
    #[zbus(property)]
    fn metered(&self) -> u32 {
        todo!()
    }

    /// NetworkingEnabled property
    #[zbus(property)]
    fn networking_enabled(&self) -> bool {
        todo!()
    }

    /// PrimaryConnection property
    #[zbus(property)]
    fn primary_connection(&self) -> OwnedObjectPath {
        todo!()
    }

    /// PrimaryConnectionType property
    #[zbus(property)]
    fn primary_connection_type(&self) -> String {
        todo!()
    }

    /// RadioFlags property
    #[zbus(property)]
    fn radio_flags(&self) -> u32 {
        todo!()
    }

    /// Startup property
    #[zbus(property)]
    fn startup(&self) -> bool {
        todo!()
    }

    // /// State property
    // #[zbus(property)]
    // fn state(&self) -> u32 {
    //     todo!()
    // }

    /// Version property
    #[zbus(property)]
    fn version(&self) -> String {
        todo!()
    }

    /// VersionInfo property
    #[zbus(property)]
    fn version_info(&self) -> Vec<u32> {
        todo!()
    }

    /// WimaxEnabled property
    #[zbus(property)]
    fn wimax_enabled(&self) -> bool {
        todo!()
    }
    // #[zbus(property)]
    // fn set_wimax_enabled(&self, value: bool) -> zbus::Result<()> {
    //     todo!()
    // }

    /// WimaxHardwareEnabled property
    #[zbus(property)]
    fn wimax_hardware_enabled(&self) -> bool {
        todo!()
    }

    /// WirelessEnabled property
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        todo!()
    }
    // #[zbus(property)]
    // fn set_wireless_enabled(&self, value: bool) -> zbus::Result<()> {
    //     todo!()
    // }

    /// WirelessHardwareEnabled property
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> bool {
        todo!()
    }

    /// WwanEnabled property
    #[zbus(property)]
    fn wwan_enabled(&self) -> bool {
        todo!()
    }
    // #[zbus(property)]
    // fn set_wwan_enabled(&self, value: bool) -> zbus::Result<()> {
    //     todo!()
    // }

    /// WwanHardwareEnabled property
    #[zbus(property)]
    fn wwan_hardware_enabled(&self) -> bool {
        todo!()
    }
}
