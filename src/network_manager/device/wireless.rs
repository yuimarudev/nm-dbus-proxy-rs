// Modified by yuimarudev on 2026-03-23.
// This file contains changes from the original upstream work.
use std::collections::HashMap;

use zbus::{
    Connection, fdo,
    fdo::Properties,
    interface,
    names::InterfaceName,
    object_server::SignalEmitter,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

use crate::{enums::NM80211Mode, runtime::Runtime};

#[derive(Clone, Debug, Default)]
pub struct DeviceWireless {
    pub access_points: Vec<OwnedObjectPath>,
    pub active_access_point: OwnedObjectPath,
    pub bitrate: u32,
    pub hw_address: String,
    pub interface_name: String,
    pub last_scan: i64,
    pub mode: NM80211Mode,
    pub perm_hw_address: String,
    pub runtime: Runtime,
    pub wireless_capabilities: u32,
}

/// see: [Device.Wireless]( https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.Wireless.html )
#[interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl DeviceWireless {
    #[zbus(signal, name = "AccessPointAdded")]
    pub(crate) async fn emit_access_point_added(
        emitter: &SignalEmitter<'_>,
        access_point: OwnedObjectPath,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "AccessPointRemoved")]
    pub(crate) async fn emit_access_point_removed(
        emitter: &SignalEmitter<'_>,
        access_point: OwnedObjectPath,
    ) -> zbus::Result<()>;

    fn get_access_points(&self) -> Vec<OwnedObjectPath> {
        self.runtime
            .wireless_device(&self.interface_name)
            .map(|record| record.access_points)
            .unwrap_or_else(|| self.access_points.clone())
    }

    fn get_all_access_points(&self) -> Vec<OwnedObjectPath> {
        self.get_access_points()
    }

    fn get_hidden_access_points(&self) -> Vec<(String, i16, String)> {
        self.runtime
            .connections_for_interface(&self.interface_name)
            .into_iter()
            .filter_map(|path| self.runtime.connection(&path))
            .filter(|connection| connection.is_hidden())
            .filter_map(|connection| {
                connection
                    .ssid()
                    .map(|ssid| (ssid, 0_i16, String::from("infrastructure")))
            })
            .collect()
    }

    fn register_signal_level_agent(&self, agent_path: OwnedObjectPath, _levels: u16) {
        self.runtime
            .add_signal_level_agent(&self.interface_name, agent_path);
    }

    fn unregister_signal_level_agent(&self, agent_path: OwnedObjectPath) {
        self.runtime
            .remove_signal_level_agent(&self.interface_name, &agent_path);
    }

    async fn request_scan(
        &self,
        _options: HashMap<String, OwnedValue>,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        let state = crate::iwd::State::request(bus)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        let station_path = state
            .device_by_name(&self.interface_name)
            .map(|device| device.path.clone())
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown iwd station")))?;
        crate::iwd::station_scan(bus, &station_path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        let last_scan = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
            .unwrap_or(0);
        let _ = self
            .runtime
            .update_wireless_device(&self.interface_name, |record| {
                record.last_scan = last_scan;
            });
        let _ = emit_wireless_property_changed(
            bus,
            &self.interface_name,
            "LastScan",
            Value::from(last_scan),
        );
        Ok(())
    }

    #[zbus(property)]
    fn access_points(&self) -> Vec<OwnedObjectPath> {
        self.runtime
            .wireless_device(&self.interface_name)
            .map(|record| record.access_points)
            .unwrap_or_else(|| self.access_points.clone())
    }

    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        self.runtime
            .wireless_device(&self.interface_name)
            .map(|record| record.active_access_point)
            .unwrap_or_else(|| self.active_access_point.clone())
    }

    #[zbus(property)]
    fn bitrate(&self) -> u32 {
        self.runtime
            .wireless_device(&self.interface_name)
            .map(|record| record.bitrate)
            .unwrap_or(self.bitrate)
    }

    #[deprecated]
    #[zbus(property)]
    fn hw_address(&self) -> String {
        self.hw_address.clone()
    }

    #[zbus(property)]
    fn last_scan(&self) -> i64 {
        self.runtime
            .wireless_device(&self.interface_name)
            .map(|record| record.last_scan)
            .unwrap_or(self.last_scan)
    }

    #[zbus(property)]
    fn mode(&self) -> u32 {
        self.mode as u32
    }

    #[zbus(property)]
    fn perm_hw_address(&self) -> String {
        self.perm_hw_address.clone()
    }

    #[zbus(property)]
    fn wireless_capabilities(&self) -> u32 {
        self.wireless_capabilities
    }
}

async fn emit_wireless_property_changed(
    bus: &Connection,
    interface_name: &str,
    property: &str,
    value: Value<'static>,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(bus, crate::device_object_path(interface_name))?;
    Properties::properties_changed(
        &emitter,
        InterfaceName::try_from("org.freedesktop.NetworkManager.Device.Wireless")
            .expect("wireless interface name should be valid"),
        HashMap::from([(property, value)]),
        Vec::<&str>::new().into(),
    )
    .await
}

pub(crate) async fn emit_access_point_added_signal(
    bus: &Connection,
    interface_name: &str,
    access_point: OwnedObjectPath,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(bus, crate::device_object_path(interface_name))?;
    DeviceWireless::emit_access_point_added(&emitter, access_point).await
}

pub(crate) async fn emit_access_point_removed_signal(
    bus: &Connection,
    interface_name: &str,
    access_point: OwnedObjectPath,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(bus, crate::device_object_path(interface_name))?;
    DeviceWireless::emit_access_point_removed(&emitter, access_point).await
}
