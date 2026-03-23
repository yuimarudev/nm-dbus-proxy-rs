// Modified by yuimarudev on 2026-03-23.
// This file contains changes from the original upstream work.
use std::sync::{Arc, Mutex, OnceLock};
use std::{collections::HashMap, time::Duration};

use futures_util::TryStreamExt;
use rtnetlink::new_connection;
use zbus::{
    Connection, ObjectServer, Proxy,
    fdo::{self, Properties},
    interface,
    names::InterfaceName,
    object_server::SignalEmitter,
    zvariant::{OwnedObjectPath, Value},
};

pub mod access_point;
pub mod active_connection;
pub mod agent_manager;
pub mod checkpoint;
pub mod device;
pub mod dns_manager;
pub mod ppp;
pub mod settings;
pub mod settings_connection;
pub mod vpn_connection;
pub mod vpn_plugin;
pub mod wifi_p2p_peer;

use crate::{
    enums::{
        NMActivationStateFlags, NMActiveConnectionState, NMConnectivityState, NMDeviceState,
        NMDeviceStateReason,
    },
    runtime::{ActiveConnectionRecord, ConnectionRecord, Runtime},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkOperation {
    Up,
    Down,
    Delete,
}

type LinkOperationOverride =
    Arc<dyn Fn(LinkOperation, &str) -> Result<(), String> + Send + Sync + 'static>;

fn link_operation_override_slot() -> &'static Mutex<Option<LinkOperationOverride>> {
    static SLOT: OnceLock<Mutex<Option<LinkOperationOverride>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

pub fn set_link_operation_override<F>(override_fn: F)
where
    F: Fn(LinkOperation, &str) -> Result<(), String> + Send + Sync + 'static,
{
    *link_operation_override_slot()
        .lock()
        .expect("link operation override mutex poisoned") = Some(Arc::new(override_fn));
}

pub fn clear_link_operation_override() {
    *link_operation_override_slot()
        .lock()
        .expect("link operation override mutex poisoned") = None;
}

pub(crate) fn maybe_run_link_operation_override(
    operation: LinkOperation,
    interface_name: &str,
) -> Option<Result<(), String>> {
    let override_fn = link_operation_override_slot()
        .lock()
        .expect("link operation override mutex poisoned")
        .clone()?;
    Some(override_fn(operation, interface_name))
}

/// see: [NetworkManager]( https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.html )
#[derive(Clone, Debug, Default)]
pub struct NetworkManager {
    pub active_connections: Vec<OwnedObjectPath>,
    pub all_devices: Vec<OwnedObjectPath>,
    pub connectivity: NMConnectivityState,
    pub global_dns_configuration: HashMap<String, zbus::zvariant::OwnedValue>,
    pub metered: u32,
    pub networking_enabled: bool,
    pub permissions: HashMap<String, String>,
    pub primary_connection: OwnedObjectPath,
    pub primary_connection_type: String,
    pub radio_flags: u32,
    pub runtime: Runtime,
    pub startup: bool,
    pub state: u32,
    pub version: String,
    pub version_info: Vec<u32>,
    pub wimax_enabled: bool,
    pub wimax_hardware_enabled: bool,
    pub wireless_enabled: bool,
    pub wireless_hardware_enabled: bool,
    pub wwan_enabled: bool,
    pub wwan_hardware_enabled: bool,
    pub devices: Vec<OwnedObjectPath>,
}

#[interface(name = "org.freedesktop.NetworkManager")]
impl NetworkManager {
    #[zbus(signal, name = "CheckPermissions")]
    pub(crate) async fn emit_check_permissions(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal, name = "StateChanged")]
    pub(crate) async fn emit_state_changed(
        emitter: &SignalEmitter<'_>,
        state: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "DeviceAdded")]
    pub(crate) async fn emit_device_added(
        emitter: &SignalEmitter<'_>,
        device_path: OwnedObjectPath,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "DeviceRemoved")]
    pub(crate) async fn emit_device_removed(
        emitter: &SignalEmitter<'_>,
        device_path: OwnedObjectPath,
    ) -> zbus::Result<()>;

    async fn activate_connection(
        &self,
        connection: OwnedObjectPath,
        device: OwnedObjectPath,
        _specific_object: OwnedObjectPath,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<OwnedObjectPath> {
        let connection_record = self
            .runtime
            .connection(&connection)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection object")))?;
        let interface_name = if device != null_path() {
            device
                .as_str()
                .rsplit('/')
                .next()
                .map(ToString::to_string)
                .ok_or_else(|| fdo::Error::Failed(String::from("invalid device object path")))?
        } else {
            connection_record.interface_name().ok_or_else(|| {
                fdo::Error::Failed(String::from("connection has no interface-name"))
            })?
        };

        if !self.runtime.networking_enabled() || self.runtime.sleeping() {
            return Err(fdo::Error::Failed(String::from("networking is disabled")));
        }
        if connection_record.connection_type == "802-11-wireless" {
            if !self.runtime.wireless_enabled() {
                return Err(fdo::Error::Failed(String::from("wireless is disabled")));
            }
            activate_wifi_connection(bus, &self.runtime, &connection_record, &interface_name)
                .await
                .map_err(fdo::Error::Failed)?;
        } else {
            activate_wired_connection(&interface_name)
                .await
                .map_err(fdo::Error::Failed)?;
        }

        let active_path = crate::active_connection_object_path(&interface_name);
        let is_primary = self.runtime.active_connection_paths().is_empty();
        let active_connection = ActiveConnectionRecord {
            path: active_path.clone(),
            value: active_connection_for(&connection_record, &interface_name, is_primary),
        };

        let _ = server
            .remove::<active_connection::ActiveConnection, _>(active_path.as_str())
            .await;
        server
            .at(active_path.as_str(), active_connection.value.clone())
            .await
            .map_err(fdo::Error::from)?;
        if connection_record.connection_type == "vpn" {
            server
                .at(
                    active_path.as_str(),
                    crate::network_manager::vpn_connection::VpnConnection::default(),
                )
                .await
                .map_err(fdo::Error::from)?;
        }
        self.runtime.add_active_connection(active_connection);
        let emitter = zbus::object_server::SignalEmitter::new(bus, active_path.as_str())
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        active_connection::ActiveConnection::emit_state_changed(
            &emitter,
            NMActiveConnectionState::Activated as u32,
            0,
        )
        .await
        .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_root_runtime_changes(bus, &self.runtime)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_device_runtime_changes(bus, &self.runtime, &interface_name)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;

        Ok(active_path)
    }

    async fn add_and_activate_connection(
        &self,
        connection: HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>>,
        device: OwnedObjectPath,
        specific_object: OwnedObjectPath,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<(OwnedObjectPath, OwnedObjectPath)> {
        let connection_path = add_runtime_connection(&self.runtime, connection, false, server, bus)
            .await
            .map_err(fdo::Error::Failed)?;
        let active_path = self
            .activate_connection(
                connection_path.clone(),
                device,
                specific_object,
                server,
                bus,
            )
            .await?;
        crate::network_manager::settings::emit_settings_changed(bus, &self.runtime, None)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_root_runtime_changes(bus, &self.runtime)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok((connection_path, active_path))
    }

    async fn add_and_activate_connection2(
        &self,
        connection: HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>>,
        device: OwnedObjectPath,
        specific_object: OwnedObjectPath,
        _options: HashMap<String, zbus::zvariant::OwnedValue>,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<(
        OwnedObjectPath,
        OwnedObjectPath,
        HashMap<String, zbus::zvariant::OwnedValue>,
    )> {
        let (connection_path, active_path) = self
            .add_and_activate_connection(connection, device, specific_object, server, bus)
            .await?;
        Ok((connection_path, active_path, HashMap::new()))
    }

    async fn checkpoint_create(
        &self,
        devices: Vec<OwnedObjectPath>,
        rollback_timeout: u32,
        _flags: u32,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<OwnedObjectPath> {
        let device_list = if devices.is_empty() {
            self.runtime.device_paths()
        } else {
            devices
        };
        let path = self.runtime.next_checkpoint_path();
        let created = current_boottime_millis();
        let mut snapshot = self.runtime.checkpoint_snapshot(&device_list);
        snapshot.persisted_files = crate::persistence::snapshot_connection_files()
            .await
            .map_err(fdo::Error::Failed)?;
        self.runtime
            .add_checkpoint(crate::runtime::CheckpointRecord {
                created,
                devices: device_list.clone(),
                path: path.clone(),
                rollback_timeout,
                rollback_deadline_millis: if rollback_timeout == 0 {
                    None
                } else {
                    Some(created.saturating_add(i64::from(rollback_timeout) * 1000))
                },
                snapshot,
            });
        server
            .at(
                path.as_str(),
                checkpoint::Checkpoint {
                    created,
                    devices: device_list,
                    path: path.clone(),
                    rollback_timeout,
                    runtime: self.runtime.clone(),
                },
            )
            .await
            .map_err(fdo::Error::from)?;
        crate::emit_object_added(bus, &path, "org.freedesktop.NetworkManager.Checkpoint")
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_root_runtime_changes(bus, &self.runtime)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        if rollback_timeout > 0 {
            spawn_checkpoint_timeout(bus.clone(), self.runtime.clone(), path.clone());
        }
        Ok(path)
    }

    async fn checkpoint_destroy(
        &self,
        checkpoint: OwnedObjectPath,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        self.runtime
            .remove_checkpoint(&checkpoint)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown checkpoint")))?;
        let _ = server
            .remove::<checkpoint::Checkpoint, _>(checkpoint.as_str())
            .await;
        crate::emit_object_removed(
            bus,
            &checkpoint,
            "org.freedesktop.NetworkManager.Checkpoint",
        )
        .await
        .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_root_runtime_changes(bus, &self.runtime)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    async fn checkpoint_adjust_rollback_timeout(
        &self,
        checkpoint: OwnedObjectPath,
        add_timeout: u32,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        let now = current_boottime_millis();
        self.runtime
            .update_checkpoint(&checkpoint, |record| {
                let baseline = record.rollback_deadline_millis.unwrap_or(now).max(now);
                let extended = baseline.saturating_add(i64::from(add_timeout) * 1000);
                record.rollback_deadline_millis = Some(extended);
                record.rollback_timeout = ((extended - now + 999) / 1000)
                    .try_into()
                    .unwrap_or(u32::MAX);
            })
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown checkpoint")))?;
        let emitter = SignalEmitter::new(bus, checkpoint.as_str())
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Properties::properties_changed(
            &emitter,
            InterfaceName::try_from("org.freedesktop.NetworkManager.Checkpoint")
                .expect("checkpoint interface name should be valid"),
            HashMap::from([(
                "RollbackTimeout",
                Value::from(
                    self.runtime
                        .checkpoint_rollback_timeout(&checkpoint, current_boottime_millis())
                        .unwrap_or(0),
                ),
            )]),
            Vec::<&str>::new().into(),
        )
        .await
        .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    async fn checkpoint_rollback(
        &self,
        checkpoint: OwnedObjectPath,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<HashMap<String, u32>> {
        let result = rollback_checkpoint(bus, &self.runtime, &checkpoint)
            .await
            .map_err(fdo::Error::Failed)?;
        let _ = server
            .remove::<checkpoint::Checkpoint, _>(checkpoint.as_str())
            .await;
        Ok(result)
    }

    fn check_connectivity(&self) -> u32 {
        connectivity_from_runtime(&self.runtime) as u32
    }

    async fn deactivate_connection(
        &self,
        active_connection: OwnedObjectPath,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        let record = self
            .runtime
            .active_connection(&active_connection)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown active connection object")))?;
        let interface_name = record
            .value
            .devices
            .first()
            .and_then(|path| path.as_str().rsplit('/').next())
            .ok_or_else(|| fdo::Error::Failed(String::from("active connection has no device")))?;

        let interface_name = interface_name.to_string();
        if record.value.type_ == "802-11-wireless" {
            deactivate_wifi_connection_with_bus(bus, &interface_name)
                .await
                .map_err(fdo::Error::Failed)?;
        } else {
            deactivate_wired_connection(&interface_name)
                .await
                .map_err(fdo::Error::Failed)?;
        }

        self.runtime.remove_active_connection(&active_connection);
        server
            .remove::<active_connection::ActiveConnection, _>(active_connection.as_str())
            .await
            .map_err(fdo::Error::from)?;
        let _ = server
            .remove::<crate::network_manager::vpn_connection::VpnConnection, _>(
                active_connection.as_str(),
            )
            .await;
        let device_path = device_path(&interface_name);
        let emitter = zbus::object_server::SignalEmitter::new(bus, device_path.as_str())
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        crate::network_manager::device::Device::emit_state_changed(
            &emitter,
            NMDeviceState::Disconnected as u32,
            NMDeviceState::Activated as u32,
            NMDeviceStateReason::UserRequested as u32,
        )
        .await
        .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_root_runtime_changes(bus, &self.runtime)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_device_runtime_changes(bus, &self.runtime, &interface_name)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    async fn enable(
        &self,
        enable: bool,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        self.runtime.set_networking_enabled(enable);
        if !enable {
            clear_active_connections_of_types(
                server,
                &self.runtime,
                &["802-11-wireless", "802-3-ethernet"],
                Some(bus),
            )
            .await
            .map_err(fdo::Error::Failed)?;
        }
        emit_root_runtime_changes(bus, &self.runtime)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    fn get_all_devices(&self) -> Vec<OwnedObjectPath> {
        let dynamic = self.runtime.device_paths();
        if dynamic.is_empty() {
            self.all_devices.clone()
        } else {
            dynamic
        }
    }

    fn get_device_by_ip_iface(&self, iface: &str) -> fdo::Result<OwnedObjectPath> {
        let devices = self.runtime.device_paths();
        let devices = if devices.is_empty() {
            self.devices.clone()
        } else {
            devices
        };
        devices
            .iter()
            .find(|path| path.as_str().ends_with(iface))
            .cloned()
            .ok_or_else(|| fdo::Error::Failed(format!("No device for interface '{iface}'")))
    }

    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        let dynamic = self.runtime.device_paths();
        if dynamic.is_empty() {
            self.devices.clone()
        } else {
            dynamic
        }
    }

    fn get_permissions(&self) -> HashMap<String, String> {
        self.permissions.clone()
    }

    fn get_logging(&self) -> (String, String) {
        self.runtime.logging()
    }

    async fn reload(&self, _flags: u32, #[zbus(connection)] bus: &Connection) -> fdo::Result<()> {
        let system_bus = zbus::conn::Builder::system()
            .map_err(|error| fdo::Error::Failed(error.to_string()))?
            .build()
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        let manager = crate::systemd_networkd::Manager::request(&system_bus)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        let wireless = crate::iwd::State::request(&system_bus)
            .await
            .unwrap_or_default();
        crate::sync_backends(bus, &self.runtime, manager, wireless)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))
    }

    fn set_logging(&self, level: &str, domains: &str) {
        self.runtime
            .set_logging(level.to_string(), domains.to_string());
    }

    async fn sleep(
        &self,
        sleep: bool,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        self.runtime.set_sleeping(sleep);
        if sleep {
            clear_active_connections_of_types(
                server,
                &self.runtime,
                &["802-11-wireless", "802-3-ethernet"],
                Some(bus),
            )
            .await
            .map_err(fdo::Error::Failed)?;
        }
        emit_root_runtime_changes(bus, &self.runtime)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    #[zbus(name = "state")]
    fn state_(&self) -> u32 {
        state_from_runtime(&self.runtime)
    }

    #[zbus(property)]
    fn activating_connection(&self) -> OwnedObjectPath {
        self.runtime
            .active_connection_paths()
            .first()
            .cloned()
            .unwrap_or_else(OwnedObjectPath::default)
    }

    #[zbus(property)]
    fn active_connections(&self) -> Vec<OwnedObjectPath> {
        self.runtime.active_connection_paths()
    }

    #[zbus(property)]
    fn all_devices(&self) -> Vec<OwnedObjectPath> {
        let dynamic = self.runtime.device_paths();
        if dynamic.is_empty() {
            self.all_devices.clone()
        } else {
            dynamic
        }
    }

    #[zbus(property)]
    fn capabilities(&self) -> Vec<u32> {
        let mut capabilities = Vec::new();
        if command_exists("teamd") {
            capabilities.push(1);
        }
        if command_exists("ovs-vsctl") || command_exists("ovsdb-client") {
            capabilities.push(2);
        }
        capabilities
    }

    #[zbus(property)]
    fn checkpoints(&self) -> Vec<OwnedObjectPath> {
        self.runtime.checkpoint_paths()
    }

    #[zbus(property)]
    fn connectivity(&self) -> u32 {
        self.check_connectivity()
    }

    #[zbus(property)]
    fn connectivity_check_available(&self) -> bool {
        !connectivity_check_uri().is_empty()
    }

    #[zbus(property)]
    fn connectivity_check_enabled(&self) -> bool {
        self.runtime.connectivity_check_enabled()
    }
    #[zbus(property)]
    async fn set_connectivity_check_enabled(
        &self,
        value: bool,
        #[zbus(connection)] bus: &Connection,
    ) -> zbus::Result<()> {
        self.runtime.set_connectivity_check_enabled(value);
        emit_root_runtime_changes(bus, &self.runtime).await
    }

    #[zbus(property)]
    fn connectivity_check_uri(&self) -> String {
        connectivity_check_uri()
    }

    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        let dynamic = self.runtime.device_paths();
        if dynamic.is_empty() {
            self.devices.clone()
        } else {
            dynamic
        }
    }

    #[zbus(property)]
    fn global_dns_configuration(&self) -> HashMap<String, zbus::zvariant::OwnedValue> {
        self.runtime.global_dns_configuration()
    }
    #[zbus(property)]
    async fn set_global_dns_configuration(
        &self,
        value: HashMap<String, zbus::zvariant::OwnedValue>,
        #[zbus(connection)] bus: &Connection,
    ) -> zbus::Result<()> {
        self.runtime.set_global_dns_configuration(value);
        emit_root_runtime_changes(bus, &self.runtime).await
    }

    #[zbus(property)]
    fn metered(&self) -> u32 {
        self.metered
    }

    #[zbus(property)]
    fn networking_enabled(&self) -> bool {
        self.runtime.networking_enabled()
    }

    #[zbus(property)]
    fn primary_connection(&self) -> OwnedObjectPath {
        self.runtime
            .active_connection_paths()
            .first()
            .cloned()
            .unwrap_or_else(OwnedObjectPath::default)
    }

    #[zbus(property)]
    fn primary_connection_type(&self) -> String {
        self.runtime
            .active_connection_paths()
            .first()
            .and_then(|path| self.runtime.active_connection(path))
            .map(|record| record.value.type_)
            .unwrap_or_else(|| self.primary_connection_type.clone())
    }

    #[zbus(property)]
    fn radio_flags(&self) -> u32 {
        self.radio_flags
    }

    #[zbus(property)]
    fn startup(&self) -> bool {
        self.startup
    }

    #[zbus(property)]
    fn state(&self) -> u32 {
        state_from_runtime(&self.runtime)
    }

    #[zbus(property)]
    fn version(&self) -> String {
        self.version.clone()
    }

    #[zbus(property)]
    fn version_info(&self) -> Vec<u32> {
        self.version_info.clone()
    }

    #[zbus(property)]
    fn wimax_enabled(&self) -> bool {
        self.runtime.wimax_enabled()
    }
    #[zbus(property)]
    async fn set_wimax_enabled(
        &self,
        value: bool,
        #[zbus(connection)] bus: &Connection,
    ) -> zbus::Result<()> {
        self.runtime.set_wimax_enabled(value);
        emit_root_runtime_changes(bus, &self.runtime).await
    }

    #[zbus(property)]
    fn wimax_hardware_enabled(&self) -> bool {
        self.wimax_hardware_enabled
    }

    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        self.runtime.wireless_enabled()
    }
    #[zbus(property)]
    async fn set_wireless_enabled(
        &self,
        value: bool,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> zbus::Result<()> {
        self.runtime.set_wireless_enabled(value);
        if !value {
            clear_active_connections_of_types(
                server,
                &self.runtime,
                &["802-11-wireless"],
                Some(bus),
            )
            .await
            .map_err(|error| zbus::Error::from(fdo::Error::Failed(error)))?;
        }
        emit_root_runtime_changes(bus, &self.runtime).await
    }

    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> bool {
        self.wireless_hardware_enabled
    }

    #[zbus(property)]
    fn wwan_enabled(&self) -> bool {
        self.runtime.wwan_enabled()
    }
    #[zbus(property)]
    async fn set_wwan_enabled(
        &self,
        value: bool,
        #[zbus(connection)] bus: &Connection,
    ) -> zbus::Result<()> {
        self.runtime.set_wwan_enabled(value);
        emit_root_runtime_changes(bus, &self.runtime).await
    }

    #[zbus(property)]
    fn wwan_hardware_enabled(&self) -> bool {
        self.wwan_hardware_enabled
    }
}

fn active_connection_for(
    connection: &ConnectionRecord,
    interface_name: &str,
    is_primary: bool,
) -> active_connection::ActiveConnection {
    active_connection::ActiveConnection {
        connection: connection.path.clone(),
        controller: null_path(),
        default: is_primary,
        default6: is_primary,
        devices: vec![device_path(interface_name)],
        dhcp4_config: null_path(),
        dhcp6_config: null_path(),
        id: connection.id(),
        ip4_config: null_path(),
        ip6_config: null_path(),
        specific_object: null_path(),
        state: NMActiveConnectionState::Activated,
        state_flags: activation_state_flags(true, false),
        type_: connection.connection_type.clone(),
        uuid: connection.uuid.clone(),
        vpn: connection.connection_type == "vpn",
    }
}

fn activation_state_flags(ip4_ready: bool, ip6_ready: bool) -> u32 {
    let mut flags = NMActivationStateFlags::Layer2Ready as u32;
    if ip4_ready {
        flags |= NMActivationStateFlags::Ip4Ready as u32;
    }
    if ip6_ready {
        flags |= NMActivationStateFlags::Ip6Ready as u32;
    }
    flags
}

async fn activate_wifi_connection(
    bus: &Connection,
    runtime: &Runtime,
    connection: &ConnectionRecord,
    interface_name: &str,
) -> Result<(), String> {
    let ssid = connection
        .ssid()
        .ok_or_else(|| String::from("Wi-Fi connection is missing SSID"))?;
    let state = crate::iwd::State::request(bus)
        .await
        .map_err(|error| error.to_string())?;
    let station_path = state
        .device_by_name(interface_name)
        .map(|device| device.path.clone())
        .ok_or_else(|| format!("unknown iwd station for interface '{interface_name}'"))?;

    if connection.is_hidden() {
        crate::iwd::station_connect_hidden(bus, &station_path, &ssid)
            .await
            .map_err(|error| error.to_string())
    } else if let Some(path) = crate::iwd::known_network_for_name(bus, &ssid)
        .await
        .map_err(|error| error.to_string())?
    {
        let _ = if connection.wifi_passphrase().is_none() {
            None
        } else {
            secret_from_agent(bus, runtime, connection).await
        };
        crate::iwd::known_network_connect(bus, &path, &station_path)
            .await
            .map_err(|error| error.to_string())
    } else {
        crate::iwd::station_connect_hidden(bus, &station_path, &ssid)
            .await
            .map_err(|error| error.to_string())
    }
}

async fn secret_from_agent(
    bus: &Connection,
    runtime: &Runtime,
    connection: &ConnectionRecord,
) -> Option<String> {
    let agent = runtime.registered_agents().last().cloned()?;
    let proxy = Proxy::new(
        bus,
        agent.sender.as_str(),
        "/org/freedesktop/NetworkManager/SecretAgent",
        "org.freedesktop.NetworkManager.SecretAgent",
    )
    .await
    .ok()?;
    let settings: HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>> = proxy
        .call(
            "GetSecrets",
            &(
                connection.settings.clone(),
                connection.path.clone(),
                String::from("802-11-wireless-security"),
                Vec::<String>::new(),
                1u32,
            ),
        )
        .await
        .ok()?;
    settings
        .get("802-11-wireless-security")
        .and_then(|section| section.get("psk"))
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| String::try_from(value).ok())
}

async fn activate_wired_connection(interface_name: &str) -> Result<(), String> {
    if let Some(result) = maybe_run_link_operation_override(LinkOperation::Up, interface_name) {
        return result;
    }
    let (connection, handle, _) = new_connection().map_err(|error| error.to_string())?;
    tokio::spawn(connection);
    let index = link_index_by_name(&handle, interface_name).await?;
    handle
        .link()
        .set(index)
        .up()
        .execute()
        .await
        .map_err(|error| error.to_string())
}

async fn deactivate_wifi_connection_with_bus(
    bus: &Connection,
    interface_name: &str,
) -> Result<(), String> {
    let state = crate::iwd::State::request(bus)
        .await
        .map_err(|error| error.to_string())?;
    let station_path = state
        .device_by_name(interface_name)
        .map(|device| device.path.clone())
        .ok_or_else(|| format!("unknown iwd station for interface '{interface_name}'"))?;
    crate::iwd::station_disconnect(bus, &station_path)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn deactivate_wifi_connection(interface_name: &str) -> Result<(), String> {
    let bus = zbus::conn::Builder::system()
        .map_err(|error| error.to_string())?
        .build()
        .await
        .map_err(|error| error.to_string())?;
    deactivate_wifi_connection_with_bus(&bus, interface_name).await
}

pub(crate) async fn deactivate_wired_connection(interface_name: &str) -> Result<(), String> {
    if let Some(result) = maybe_run_link_operation_override(LinkOperation::Down, interface_name) {
        return result;
    }
    let (connection, handle, _) = new_connection().map_err(|error| error.to_string())?;
    tokio::spawn(connection);
    let index = link_index_by_name(&handle, interface_name).await?;
    handle
        .link()
        .set(index)
        .down()
        .execute()
        .await
        .map_err(|error| error.to_string())
}

fn connectivity_check_uri() -> String {
    crate::config::current().connectivity_check_uri
}

fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .into_iter()
        .flatten()
        .map(|dir| dir.join(name))
        .any(|path| path.is_file())
}

pub(crate) async fn link_index_by_name(
    handle: &rtnetlink::Handle,
    interface_name: &str,
) -> Result<u32, String> {
    let mut links = handle
        .link()
        .get()
        .match_name(interface_name.to_string())
        .execute();
    let Some(link) = links.try_next().await.map_err(|error| error.to_string())? else {
        return Err(format!("unknown link '{interface_name}'"));
    };
    Ok(link.header.index)
}

pub(crate) fn current_boottime_millis() -> i64 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|contents| contents.split_whitespace().next().map(ToString::to_string))
        .and_then(|value| value.parse::<f64>().ok())
        .map(|seconds| (seconds * 1000.0).round() as i64)
        .unwrap_or_default()
}

fn spawn_checkpoint_timeout(bus: Connection, runtime: Runtime, path: OwnedObjectPath) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let Some(timeout) =
                runtime.checkpoint_rollback_timeout(&path, current_boottime_millis())
            else {
                break;
            };
            if timeout == 0 {
                let _ = rollback_checkpoint(&bus, &runtime, &path).await;
                break;
            }
        }
    });
}

async fn rollback_checkpoint(
    bus: &Connection,
    runtime: &Runtime,
    checkpoint: &OwnedObjectPath,
) -> Result<HashMap<String, u32>, String> {
    let server = bus.object_server();
    let current_active = runtime
        .active_connection_paths()
        .into_iter()
        .filter_map(|path| runtime.active_connection(&path))
        .collect::<Vec<_>>();
    let record = runtime
        .remove_checkpoint(checkpoint)
        .ok_or_else(|| String::from("unknown checkpoint"))?;
    let _ = server
        .remove::<checkpoint::Checkpoint, _>(checkpoint.as_str())
        .await;
    let _ =
        crate::emit_object_removed(bus, checkpoint, "org.freedesktop.NetworkManager.Checkpoint")
            .await;
    crate::persistence::restore_connection_files(&record.snapshot.persisted_files).await?;
    runtime.restore_checkpoint_snapshot(&record.snapshot);

    let target_active = record.snapshot.active_connections.clone();
    for active in &current_active {
        let interface_name = active
            .value
            .devices
            .first()
            .and_then(|path| path.as_str().rsplit('/').next())
            .unwrap_or_default()
            .to_string();
        let still_active = target_active.iter().any(|candidate| {
            candidate
                .value
                .devices
                .first()
                .and_then(|path| path.as_str().rsplit('/').next())
                == Some(interface_name.as_str())
        });
        if still_active {
            continue;
        }
        match active.value.type_.as_str() {
            "802-11-wireless" => {
                let _ = deactivate_wifi_connection(&interface_name).await;
            }
            _ => {
                let _ = deactivate_wired_connection(&interface_name).await;
            }
        }
    }

    for active in &target_active {
        let interface_name = active
            .value
            .devices
            .first()
            .and_then(|path| path.as_str().rsplit('/').next())
            .unwrap_or_default()
            .to_string();
        let already_active = current_active.iter().any(|candidate| {
            candidate
                .value
                .devices
                .first()
                .and_then(|path| path.as_str().rsplit('/').next())
                == Some(interface_name.as_str())
        });
        if already_active {
            continue;
        }
        if let Some(connection) = runtime.connection(&active.value.connection) {
            match connection.connection_type.as_str() {
                "802-11-wireless" => {
                    let _ =
                        activate_wifi_connection(bus, runtime, &connection, &interface_name).await;
                }
                _ => {
                    let _ = activate_wired_connection(&interface_name).await;
                }
            }
        }
    }

    let active_paths = runtime.active_connection_paths();
    for active_path in active_paths {
        let _ = server
            .remove::<active_connection::ActiveConnection, _>(active_path.as_str())
            .await;
        let _ = server
            .remove::<crate::network_manager::vpn_connection::VpnConnection, _>(
                active_path.as_str(),
            )
            .await;
    }
    for active_path in runtime.active_connection_paths() {
        if let Some(record) = runtime.active_connection(&active_path) {
            let is_vpn = record.value.type_ == "vpn";
            let _ = server.at(active_path.as_str(), record.value).await;
            if is_vpn {
                let _ = server
                    .at(
                        active_path.as_str(),
                        crate::network_manager::vpn_connection::VpnConnection::default(),
                    )
                    .await;
            }
        }
    }

    emit_root_runtime_changes(bus, runtime)
        .await
        .map_err(|error| error.to_string())?;

    Ok(record
        .devices
        .into_iter()
        .map(|path| (path.as_str().to_string(), 0_u32))
        .collect())
}

fn is_networkd_connection_type(connection_type: &str) -> bool {
    connection_type != "802-11-wireless"
}

fn connectivity_from_runtime(runtime: &Runtime) -> NMConnectivityState {
    if runtime.sleeping()
        || !runtime.networking_enabled()
        || runtime.active_connection_paths().is_empty()
    {
        NMConnectivityState::None
    } else if !runtime.connectivity_check_enabled() || connectivity_check_uri().is_empty() {
        NMConnectivityState::Full
    } else if connectivity_probe(connectivity_check_uri().as_str()) {
        NMConnectivityState::Full
    } else {
        NMConnectivityState::Limited
    }
}

fn connectivity_probe(uri: &str) -> bool {
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };

    match client.head(uri).send() {
        Ok(response) => response.status().is_success() || response.status().is_redirection(),
        Err(_) => client
            .get(uri)
            .send()
            .map(|response| response.status().is_success() || response.status().is_redirection())
            .unwrap_or(false),
    }
}

pub(crate) async fn emit_root_runtime_changes(
    bus: &Connection,
    runtime: &Runtime,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(bus, "/org/freedesktop/NetworkManager")?;
    let primary_connection = runtime
        .active_connection_paths()
        .first()
        .cloned()
        .unwrap_or_else(OwnedObjectPath::default);
    let primary_connection_type = runtime
        .active_connection_paths()
        .first()
        .and_then(|path| runtime.active_connection(path))
        .map(|record| record.value.type_)
        .unwrap_or_default();
    let state = state_from_runtime(runtime);
    let changed = HashMap::from([
        (
            "ActiveConnections",
            Value::from(runtime.active_connection_paths()),
        ),
        (
            "ActivatingConnection",
            Value::from(primary_connection.clone()),
        ),
        ("AllDevices", Value::from(runtime.device_paths())),
        ("Checkpoints", Value::from(runtime.checkpoint_paths())),
        (
            "Connectivity",
            Value::from(connectivity_from_runtime(runtime) as u32),
        ),
        (
            "ConnectivityCheckEnabled",
            Value::from(runtime.connectivity_check_enabled()),
        ),
        ("Devices", Value::from(runtime.device_paths())),
        (
            "GlobalDnsConfiguration",
            Value::from(runtime.global_dns_configuration()),
        ),
        (
            "NetworkingEnabled",
            Value::from(runtime.networking_enabled()),
        ),
        ("PrimaryConnection", Value::from(primary_connection)),
        (
            "PrimaryConnectionType",
            Value::from(primary_connection_type),
        ),
        ("State", Value::from(state)),
        ("WimaxEnabled", Value::from(runtime.wimax_enabled())),
        ("WirelessEnabled", Value::from(runtime.wireless_enabled())),
        ("WwanEnabled", Value::from(runtime.wwan_enabled())),
    ]);
    Properties::properties_changed(
        &emitter,
        InterfaceName::try_from("org.freedesktop.NetworkManager")
            .expect("root interface name should be valid"),
        changed,
        Vec::<&str>::new().into(),
    )
    .await?;
    NetworkManager::emit_state_changed(&emitter, state).await
}

pub(crate) async fn emit_device_runtime_changes(
    bus: &Connection,
    runtime: &Runtime,
    interface_name: &str,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(bus, crate::device_object_path(interface_name))?;
    let active_connection = runtime
        .active_connection_for_interface(interface_name)
        .unwrap_or_else(OwnedObjectPath::default);
    let changed = HashMap::from([("ActiveConnection", Value::from(active_connection))]);
    Properties::properties_changed(
        &emitter,
        InterfaceName::try_from("org.freedesktop.NetworkManager.Device")
            .expect("device interface name should be valid"),
        changed,
        Vec::<&str>::new().into(),
    )
    .await
}

fn device_path(interface_name: &str) -> OwnedObjectPath {
    crate::device_object_path(interface_name)
}

fn null_path() -> OwnedObjectPath {
    OwnedObjectPath::default()
}

fn state_from_runtime(runtime: &Runtime) -> u32 {
    if runtime.sleeping() {
        10
    } else if !runtime.networking_enabled() || runtime.active_connection_paths().is_empty() {
        20
    } else {
        70
    }
}

async fn add_runtime_connection(
    runtime: &Runtime,
    connection: HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>>,
    unsaved: bool,
    server: &ObjectServer,
    bus: &Connection,
) -> Result<OwnedObjectPath, String> {
    let uuid = connection
        .get("connection")
        .and_then(|settings| settings.get("uuid"))
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| String::try_from(value).ok())
        .ok_or_else(|| String::from("missing connection.uuid"))?;
    let connection_type = connection
        .get("connection")
        .and_then(|settings| settings.get("type"))
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| String::try_from(value).ok())
        .ok_or_else(|| String::from("missing connection.type"))?;
    let path = runtime.next_connection_path();
    let mut record = ConnectionRecord {
        connection_type,
        filename: String::new(),
        flags: 0,
        origin: crate::runtime::ConnectionOrigin::User,
        path: path.clone(),
        settings: connection,
        unsaved,
        uuid,
    };
    if !unsaved {
        crate::persistence::persist_connection(&mut record).await?;
    }
    runtime.add_connection(record);
    server
        .at(
            path.as_str(),
            crate::network_manager::settings_connection::SettingsConnection {
                path: path.clone(),
                runtime: runtime.clone(),
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    crate::emit_object_added(
        bus,
        &path,
        "org.freedesktop.NetworkManager.Settings.Connection",
    )
    .await
    .map_err(|error| error.to_string())?;
    crate::network_manager::settings::emit_new_connection_signal(bus, &path)
        .await
        .map_err(|error| error.to_string())?;
    Ok(path)
}

async fn clear_active_connections(server: &ObjectServer, runtime: &Runtime) -> Result<(), String> {
    clear_active_connections_of_types(
        server,
        runtime,
        &["802-11-wireless", "802-3-ethernet"],
        None,
    )
    .await?;
    Ok(())
}

async fn clear_active_connections_of_types(
    server: &ObjectServer,
    runtime: &Runtime,
    connection_types: &[&str],
    bus: Option<&Connection>,
) -> Result<Vec<String>, String> {
    let mut interfaces = Vec::new();
    let active_paths = runtime.active_connection_paths();
    for active_path in active_paths {
        if let Some(record) = runtime.active_connection(&active_path) {
            if !connection_types
                .iter()
                .any(|kind| *kind == record.value.type_)
            {
                continue;
            }
            if let Some(interface_name) = record
                .value
                .devices
                .first()
                .and_then(|path| path.as_str().rsplit('/').next())
                .map(ToString::to_string)
            {
                interfaces.push(interface_name.clone());
                match record.value.type_.as_str() {
                    "802-11-wireless" => {
                        if let Some(bus) = bus {
                            deactivate_wifi_connection_with_bus(bus, &interface_name).await?
                        } else {
                            deactivate_wifi_connection(&interface_name).await?
                        }
                    }
                    "802-3-ethernet" => deactivate_wired_connection(&interface_name).await?,
                    _ => {}
                }
            }
        }
        runtime.remove_active_connection(&active_path);
        let _ = server
            .remove::<active_connection::ActiveConnection, _>(active_path.as_str())
            .await;
        let _ = server
            .remove::<crate::network_manager::vpn_connection::VpnConnection, _>(
                active_path.as_str(),
            )
            .await;
    }
    Ok(interfaces)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::connectivity_probe;

    #[test]
    fn connectivity_probe_accepts_successful_http_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener should bind");
        let address = listener
            .local_addr()
            .expect("listener should expose local address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("server should accept a request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            stream
                .write_all(
                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .expect("server should write a response");
        });

        assert!(connectivity_probe(format!("http://{address}/").as_str()));
        server.join().expect("server thread should exit cleanly");
    }

    #[test]
    fn connectivity_probe_rejects_unreachable_endpoint() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener should bind");
        let address = listener
            .local_addr()
            .expect("listener should expose local address");
        drop(listener);

        assert!(!connectivity_probe(format!("http://{address}/").as_str()));
    }
}
