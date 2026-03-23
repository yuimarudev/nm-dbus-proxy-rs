use std::collections::HashMap;

use anyhow::Result;
use zbus::{
    Connection, Proxy,
    fdo::ObjectManagerProxy,
    names::OwnedInterfaceName,
    zvariant::{OwnedObjectPath, OwnedValue},
};

const DESTINATION: &str = "net.connman.iwd";
const ROOT_PATH: &str = "/";
const KNOWN_NETWORK_INTERFACE: &str = "net.connman.iwd.KnownNetwork";
const NETWORK_INTERFACE: &str = "net.connman.iwd.Network";
const DEVICE_INTERFACE: &str = "net.connman.iwd.Device";
const STATION_INTERFACE: &str = "net.connman.iwd.Station";
const BASIC_SERVICE_SET_INTERFACE: &str = "net.connman.iwd.BasicServiceSet";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BasicServiceSet {
    pub path: OwnedObjectPath,
    pub address: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Device {
    pub path: OwnedObjectPath,
    pub name: String,
    pub address: String,
    pub powered: bool,
    pub adapter: OwnedObjectPath,
    pub mode: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KnownNetwork {
    pub path: OwnedObjectPath,
    pub name: String,
    pub kind: String,
    pub hidden: bool,
    pub auto_connect: bool,
    pub last_connected_time: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Network {
    pub path: OwnedObjectPath,
    pub name: String,
    pub connected: bool,
    pub device: OwnedObjectPath,
    pub kind: String,
    pub known_network: OwnedObjectPath,
    pub extended_service_set: Vec<OwnedObjectPath>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OrderedNetwork {
    pub path: OwnedObjectPath,
    pub signal: i16,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Station {
    pub path: OwnedObjectPath,
    pub scanning: bool,
    pub state: String,
    pub connected_network: Option<OwnedObjectPath>,
    pub ordered_networks: Vec<OrderedNetwork>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct State {
    pub basic_service_sets: Vec<BasicServiceSet>,
    pub devices: Vec<Device>,
    pub known_networks: Vec<KnownNetwork>,
    pub networks: Vec<Network>,
    pub stations: Vec<Station>,
}

impl State {
    pub async fn request(conn: &Connection) -> Result<Self> {
        let object_manager = ObjectManagerProxy::builder(conn)
            .destination(DESTINATION)?
            .path(ROOT_PATH)?
            .build()
            .await?;
        let managed_objects = object_manager.get_managed_objects().await?;

        let mut state = Self::default();

        for (path, interfaces) in &managed_objects {
            if let Some(properties) = interface_properties(interfaces, BASIC_SERVICE_SET_INTERFACE) {
                state.basic_service_sets.push(BasicServiceSet {
                    path: path.clone(),
                    address: property::<String>(properties, "Address").unwrap_or_default(),
                });
            }

            if let Some(properties) = interface_properties(interfaces, DEVICE_INTERFACE) {
                state.devices.push(Device {
                    path: path.clone(),
                    name: property::<String>(properties, "Name").unwrap_or_default(),
                    address: property::<String>(properties, "Address").unwrap_or_default(),
                    powered: property::<bool>(properties, "Powered").unwrap_or_default(),
                    adapter: property::<OwnedObjectPath>(properties, "Adapter")
                        .unwrap_or_else(default_object_path),
                    mode: property::<String>(properties, "Mode").unwrap_or_default(),
                });
            }

            if let Some(properties) = interface_properties(interfaces, KNOWN_NETWORK_INTERFACE) {
                state.known_networks.push(KnownNetwork {
                    path: path.clone(),
                    name: property::<String>(properties, "Name").unwrap_or_default(),
                    kind: property::<String>(properties, "Type").unwrap_or_default(),
                    hidden: property::<bool>(properties, "Hidden").unwrap_or_default(),
                    auto_connect: property::<bool>(properties, "AutoConnect").unwrap_or_default(),
                    last_connected_time: property::<String>(properties, "LastConnectedTime")
                        .unwrap_or_default(),
                });
            }

            if let Some(properties) = interface_properties(interfaces, NETWORK_INTERFACE) {
                state.networks.push(Network {
                    path: path.clone(),
                    name: property::<String>(properties, "Name").unwrap_or_default(),
                    connected: property::<bool>(properties, "Connected").unwrap_or_default(),
                    device: property::<OwnedObjectPath>(properties, "Device")
                        .unwrap_or_else(default_object_path),
                    kind: property::<String>(properties, "Type").unwrap_or_default(),
                    known_network: property::<OwnedObjectPath>(properties, "KnownNetwork")
                        .unwrap_or_else(default_object_path),
                    extended_service_set: property::<Vec<OwnedObjectPath>>(
                        properties,
                        "ExtendedServiceSet",
                    )
                    .unwrap_or_default(),
                });
            }

            if let Some(properties) = interface_properties(interfaces, STATION_INTERFACE) {
                let station_proxy = Proxy::new(conn, DESTINATION, path.as_str(), STATION_INTERFACE).await?;
                let ordered_networks = station_proxy
                    .call::<_, _, Vec<(OwnedObjectPath, i16)>>("GetOrderedNetworks", &())
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(path, signal)| OrderedNetwork { path, signal })
                    .collect();

                state.stations.push(Station {
                    path: path.clone(),
                    scanning: property::<bool>(properties, "Scanning").unwrap_or_default(),
                    state: property::<String>(properties, "State").unwrap_or_default(),
                    connected_network: station_proxy.get_property("ConnectedNetwork").await.ok(),
                    ordered_networks,
                });
            }
        }

        Ok(state)
    }

    pub fn basic_service_set_by_path(&self, path: &OwnedObjectPath) -> Option<&BasicServiceSet> {
        self.basic_service_sets.iter().find(|bss| &bss.path == path)
    }

    pub fn device_by_name(&self, name: &str) -> Option<&Device> {
        self.devices.iter().find(|device| device.name == name)
    }

    pub fn known_network_by_path(&self, path: &OwnedObjectPath) -> Option<&KnownNetwork> {
        self.known_networks.iter().find(|network| &network.path == path)
    }

    pub fn network_by_path(&self, path: &OwnedObjectPath) -> Option<&Network> {
        self.networks.iter().find(|network| &network.path == path)
    }

    pub fn station_by_device_path(&self, device_path: &OwnedObjectPath) -> Option<&Station> {
        self.stations.iter().find(|station| station.path == *device_path)
    }
}

fn default_object_path() -> OwnedObjectPath {
    OwnedObjectPath::try_from("/").expect("root object path should be valid")
}

fn interface_properties<'a>(
    interfaces: &'a HashMap<OwnedInterfaceName, HashMap<String, OwnedValue>>,
    interface: &str,
) -> Option<&'a HashMap<String, OwnedValue>> {
    interfaces.iter().find_map(|(name, properties)| {
        if name.as_str() == interface {
            Some(properties)
        } else {
            None
        }
    })
}

fn property<T>(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    properties
        .get(name)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| T::try_from(value).ok())
}
