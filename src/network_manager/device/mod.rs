// Modified by yuimarudev on 2026-03-23.
// This file contains changes from the original upstream work.
use std::collections::HashMap;

use zbus::{
    Connection, ObjectServer,
    fdo::{self, Properties},
    interface,
    names::InterfaceName,
    object_server::SignalEmitter,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

pub mod loopback;
pub mod specialized;
pub mod wired;
pub mod wireless;

use crate::enums::{
    NMConnectivityState, NMDeviceInterfaceFlags, NMDeviceState, NMDeviceStateReason, NMDeviceType,
    NMMetered,
};
use crate::runtime::Runtime;

/// see: [Device]( https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.html )
#[derive(Clone, Debug, Default)]
pub struct Device {
    pub active_connection: OwnedObjectPath,
    pub autoconnect: bool,
    pub available_connections: Vec<OwnedObjectPath>,
    pub capabilities: u32,
    pub dhcp4_config: OwnedObjectPath,
    pub dhcp6_config: OwnedObjectPath,
    pub driver: String,
    pub driver_version: String,
    pub firmware_missing: bool,
    pub firmware_version: String,
    pub hw_address: String,
    pub interface: String,
    pub interface_flags: NMDeviceInterfaceFlags,
    pub ip4_config: OwnedObjectPath,
    pub ip4_connectivity: NMConnectivityState,
    pub ip6_config: OwnedObjectPath,
    pub ip6_connectivity: NMConnectivityState,
    pub ip_interface: String,
    pub lldp_neighbors: Vec<HashMap<String, OwnedValue>>,
    pub managed: bool,
    pub metered: NMMetered,
    pub mtu: u32,
    pub nm_plugin_missing: bool,
    pub path: String,
    pub physical_port_id: String,
    pub ports: Vec<OwnedObjectPath>,
    pub real: bool,
    pub state: NMDeviceState,
    pub state_reason: (NMDeviceState, NMDeviceStateReason),
    pub r#type: NMDeviceType,
    pub runtime: Runtime,
    pub udi: String,
}

#[interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(signal, name = "StateChanged")]
    pub(crate) async fn emit_state_changed(
        emitter: &SignalEmitter<'_>,
        new_state: u32,
        old_state: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(name = "Delete")]
    async fn delete_(
        &self,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] _bus: &Connection,
    ) -> fdo::Result<()> {
        let output = tokio::process::Command::new(crate::network_manager::networkctl_bin())
            .arg("delete")
            .arg(&self.interface)
            .output()
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        if !output.status.success() {
            return Err(fdo::Error::Failed(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }

        for connection in self.runtime.connections_for_interface(&self.interface) {
            self.runtime.remove_connection(&connection);
            let _ = server
                .remove::<crate::network_manager::settings_connection::SettingsConnection, _>(
                    connection.as_str(),
                )
                .await;
        }
        Ok(())
    }

    #[zbus(name = "Disconnect")]
    async fn disconnect_(
        &self,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        let Some(active_connection) = self.runtime.active_connection_for_interface(&self.interface) else {
            return Ok(());
        };
        if let Some(record) = self.runtime.active_connection(&active_connection) {
            match record.value.type_.as_str() {
                "802-11-wireless" => crate::network_manager::deactivate_wifi_connection(&self.interface)
                    .await
                    .map_err(fdo::Error::Failed)?,
                "802-3-ethernet" => {
                    let output = tokio::process::Command::new(crate::network_manager::networkctl_bin())
                        .arg("down")
                        .arg(&self.interface)
                        .output()
                        .await
                        .map_err(|error| fdo::Error::Failed(error.to_string()))?;
                    if !output.status.success() {
                        return Err(fdo::Error::Failed(
                            String::from_utf8_lossy(&output.stderr).trim().to_string(),
                        ));
                    }
                }
                _ => {}
            }
        }
        self.runtime.remove_active_connection(&active_connection);
        let _ = server
            .remove::<crate::network_manager::active_connection::ActiveConnection, _>(
                active_connection.as_str(),
            )
            .await;
        emit_device_property_changed(
            bus,
            &self.interface,
            "ActiveConnection",
            Value::from(OwnedObjectPath::default()),
        )
        .await
        .map_err(fdo::Error::from)?;
        Self::emit_state_changed(
            &SignalEmitter::new(
                bus,
                format!("/org/freedesktop/NetworkManager/Devices/{}", self.interface),
            )
            .map_err(|error| fdo::Error::Failed(error.to_string()))?,
            NMDeviceState::Disconnected as u32,
            NMDeviceState::Activated as u32,
            NMDeviceStateReason::UserRequested as u32,
        )
        .await
        .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        crate::network_manager::emit_root_runtime_changes(bus, &self.runtime)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    fn get_applied_connection(
        &self,
        _flags: u32,
    ) -> fdo::Result<(
        std::collections::HashMap<
            String,
            std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        >,
        u64,
    )> {
        let connection = self
            .runtime
            .active_connection_for_interface(&self.interface)
            .and_then(|path| self.runtime.active_connection(&path))
            .and_then(|active| self.runtime.connection(&active.value.connection))
            .or_else(|| {
                self.runtime
                    .connections_for_interface(&self.interface)
                    .first()
                    .cloned()
                    .and_then(|path| self.runtime.connection(&path))
            })
            .ok_or_else(|| fdo::Error::Failed(String::from("no applied connection")))?;

        Ok((connection.settings, self.runtime.version_id()))
    }

    async fn reapply(
        &self,
        connection: std::collections::HashMap<
            String,
            std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        >,
        _version_id: u64,
        _flags: u32,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        let target = self
            .runtime
            .active_connection_for_interface(&self.interface)
            .and_then(|active_path| self.runtime.active_connection(&active_path))
            .map(|active| active.value.connection)
            .or_else(|| {
                self.runtime
                    .connections_for_interface(&self.interface)
                    .first()
                    .cloned()
            })
            .ok_or_else(|| fdo::Error::Failed(String::from("no connection to reapply")))?;

        self.runtime
            .update_connection(&target, |record| {
                record.with_settings(connection);
            })
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        crate::network_manager::settings_connection::persist_runtime_connection(
            &self.runtime,
            &target,
        )
        .await
        .map_err(fdo::Error::Failed)?;
        crate::network_manager::settings_connection::emit_connection_changed(
            bus,
            &self.runtime,
            &target,
        )
        .await
        .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        crate::network_manager::settings::emit_settings_changed(bus, &self.runtime, None)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    /// `ActiveConnection` property
    #[zbus(property)]
    fn active_connection(&self) -> OwnedObjectPath {
        self.runtime
            .active_connection_for_interface(&self.interface)
            .unwrap_or_else(|| self.active_connection.clone())
    }

    /// Autoconnect property
    #[zbus(property)]
    fn autoconnect(&self) -> bool {
        self.runtime
            .connections_for_interface(&self.interface)
            .first()
            .cloned()
            .and_then(|path| self.runtime.connection(&path))
            .map(|record| record.autoconnect())
            .unwrap_or(self.autoconnect)
    }
    #[zbus(property)]
    async fn set_autoconnect(
        &self,
        value: bool,
        #[zbus(connection)] bus: &Connection,
    ) -> zbus::Result<()> {
        for connection_path in self.runtime.connections_for_interface(&self.interface) {
            let _ = self.runtime.update_connection(&connection_path, |record| {
                record
                    .settings
                    .entry(String::from("connection"))
                    .or_default()
                    .insert(String::from("autoconnect"), OwnedValue::from(value));
            });
            crate::network_manager::settings_connection::persist_runtime_connection(
                &self.runtime,
                &connection_path,
            )
            .await
            .map_err(|error| zbus::Error::from(fdo::Error::Failed(error)))?;
            crate::network_manager::settings_connection::emit_connection_changed(
                bus,
                &self.runtime,
                &connection_path,
            )
            .await?;
        }
        emit_device_property_changed(bus, &self.interface, "Autoconnect", Value::from(value))
            .await?;
        crate::network_manager::settings::emit_settings_changed(bus, &self.runtime, None).await?;
        Ok(())
    }

    /// `AvailableConnections` property
    #[zbus(property)]
    fn available_connections(&self) -> Vec<OwnedObjectPath> {
        let dynamic = self.runtime.connections_for_interface(&self.interface);
        if dynamic.is_empty() {
            self.available_connections.clone()
        } else {
            dynamic
        }
    }

    /// Capabilities property
    #[zbus(property)]
    fn capabilities(&self) -> u32 {
        self.capabilities
    }

    /// `DeviceType` property
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        self.r#type as u32
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

    /// Driver property
    #[zbus(property)]
    fn driver(&self) -> String {
        self.driver.clone()
    }

    /// `DriverVersion` property
    #[zbus(property)]
    fn driver_version(&self) -> String {
        self.driver_version.clone()
    }

    /// `FirmwareMissing` property
    #[zbus(property)]
    fn firmware_missing(&self) -> bool {
        self.firmware_missing
    }

    /// `FirmwareVersion` property
    #[zbus(property)]
    fn firmware_version(&self) -> String {
        self.firmware_version.clone()
    }

    /// `HwAddress` property
    #[zbus(property)]
    fn hw_address(&self) -> String {
        self.hw_address.clone()
    }

    /// Interface property
    #[zbus(property)]
    fn interface(&self) -> String {
        self.interface.clone()
    }

    /// `InterfaceFlags` property
    #[zbus(property)]
    fn interface_flags(&self) -> u32 {
        self.interface_flags as u32
    }

    /// `Ip4Config` property
    #[zbus(property)]
    fn ip4_config(&self) -> OwnedObjectPath {
        self.ip4_config.clone()
    }

    /// `Ip4Connectivity` property
    #[zbus(property)]
    fn ip4_connectivity(&self) -> u32 {
        self.ip4_connectivity as u32
    }

    /// `Ip6Config` property
    #[zbus(property)]
    fn ip6_config(&self) -> OwnedObjectPath {
        self.ip6_config.clone()
    }

    /// `Ip6Connectivity` property
    #[zbus(property)]
    fn ip6_connectivity(&self) -> u32 {
        self.ip6_connectivity as u32
    }

    /// `IpInterface` property
    #[zbus(property)]
    fn ip_interface(&self) -> String {
        self.ip_interface.clone()
    }

    /// `LldpNeighbors` property
    #[zbus(property)]
    fn lldp_neighbors(&self) -> Vec<HashMap<String, OwnedValue>> {
        self.lldp_neighbors.clone()
    }

    /// Managed property
    #[zbus(property)]
    fn managed(&self) -> bool {
        self.runtime
            .device_managed(&self.interface)
            .unwrap_or(self.managed)
    }
    #[zbus(property)]
    async fn set_managed(
        &self,
        value: bool,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> zbus::Result<()> {
        self.runtime.set_device_managed(&self.interface, value);
        if !value {
            let _ = self.disconnect_(server, bus).await;
        }
        emit_device_property_changed(bus, &self.interface, "Managed", Value::from(value)).await?;
        Ok(())
    }

    /// Metered property
    #[zbus(property)]
    fn metered(&self) -> u32 {
        self.metered as u32
    }

    /// Mtu property
    #[zbus(property)]
    fn mtu(&self) -> u32 {
        self.mtu
    }

    /// `NmPluginMissing` property
    #[zbus(property)]
    fn nm_plugin_missing(&self) -> bool {
        self.nm_plugin_missing
    }

    /// Path property
    #[zbus(property)]
    fn path(&self) -> String {
        self.path.clone()
    }

    /// `PhysicalPortId` property
    #[zbus(property)]
    fn physical_port_id(&self) -> String {
        self.physical_port_id.clone()
    }

    /// Ports property
    #[zbus(property)]
    fn ports(&self) -> Vec<OwnedObjectPath> {
        self.ports.clone()
    }

    /// Real property
    #[zbus(property)]
    fn real(&self) -> bool {
        self.real
    }

    /// State property
    #[zbus(property)]
    fn state(&self) -> u32 {
        if self.runtime.active_connection_for_interface(&self.interface).is_some() {
            NMDeviceState::Activated as u32
        } else {
            self.state as u32
        }
    }

    /// `StateReason` property
    #[zbus(property)]
    fn state_reason(&self) -> (u32, u32) {
        if self.runtime.active_connection_for_interface(&self.interface).is_some() {
            (NMDeviceState::Activated as u32, NMDeviceStateReason::None as u32)
        } else {
            (self.state_reason.0 as u32, self.state_reason.1 as u32)
        }
    }

    /// Udi property
    #[zbus(property)]
    fn udi(&self) -> String {
        self.udi.clone()
    }
}

async fn emit_device_property_changed(
    bus: &Connection,
    interface_name: &str,
    property: &str,
    value: Value<'static>,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(
        bus,
        crate::device_object_path(interface_name),
    )?;
    Properties::properties_changed(
        &emitter,
        InterfaceName::try_from("org.freedesktop.NetworkManager.Device")
            .expect("device interface name should be valid"),
        HashMap::from([(property, value)]),
        Vec::<&str>::new().into(),
    )
    .await
}
