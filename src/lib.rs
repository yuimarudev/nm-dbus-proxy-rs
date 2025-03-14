use std::collections::HashMap;

use anyhow::Result;
use network_manager::{
    NetworkManager,
    access_point::AccessPoint,
    active_connection::ActiveConnection,
    device::{Device, loopback::DeviceLoopback, wired::DeviceWired, wireless::DeviceWireless},
};
use systemd_networkd::Manager;
use zbus::{
    Address, Connection,
    conn::Builder,
    interface,
    zvariant::{ObjectPath, OwnedObjectPath},
};

mod enums;
mod network_manager;
pub mod systemd_networkd;

use enums::{NMDeviceState, NMDeviceStateReason, NMDeviceType};

pub async fn start_service(address: Option<Address>, manager: Manager) -> Result<Connection> {
    let ip4 = Ip4Config;

    let service_bus = if let Some(some) = address {
        Builder::address(some)?
    } else {
        Builder::system()?
    };

    let conn = service_bus.build().await?;

    let server = conn.object_server();

    let mut active_connections = vec![];
    let mut devices = vec![];
    for link in manager.links {
        let connection_path = format!(
            "/org/freedesktop/NetworkManager/ActiveConnections/{}",
            link.description.name
        );
        let connection_object_path = OwnedObjectPath::from(
            ObjectPath::try_from(connection_path.as_str())
                .expect("should parse active connection object path"),
        );

        let device_path = format!(
            "/org/freedesktop/NetworkManager/Devices/{}",
            link.description.name
        );
        let device_object_path = OwnedObjectPath::from(
            ObjectPath::try_from(device_path.as_str()).expect("should parse device object path"),
        );

        server
            .at(
                connection_path.as_str(),
                dbg!(ActiveConnection {
                    devices: vec![device_object_path.clone()],
                    id: link.description.name.clone(),
                }),
            )
            .await?;

        let device_type = NMDeviceType::from((link.description.kind, link.description.r#type));
        server
            .at(
                device_path.as_str(),
                dbg!(Device {
                    active_connection: connection_object_path.clone(),
                    driver: link.description.driver.clone(),
                    interface: link.description.name.clone(),
                    ip_interface: link.description.name.clone(),
                    mtu: link.description.mtu,
                    path: link.description.name.clone(),
                    state: NMDeviceState::Activated,
                    state_reason: (NMDeviceState::Activated, NMDeviceStateReason::None),
                    r#type: device_type,
                    udi: link.description.name.clone(),
                }),
            )
            .await?;

        match device_type {
            NMDeviceType::Loopback => {
                server.at(device_path.as_str(), DeviceLoopback).await?;
            }
            NMDeviceType::Ethernet => {
                server.at(device_path.as_str(), DeviceWired).await?;
            }
            NMDeviceType::Wifi => {
                server.at(device_path.as_str(), DeviceWireless).await?;
            }
            _ => {
                // TODO
            }
        }

        active_connections.push(connection_object_path);
        devices.push(device_object_path);
    }

    server
        .at(
            "/org/freedesktop/NetworkManager",
            NetworkManager {
                active_connections,
                devices,
            },
        )
        .await?;

    server
        .at(
            "/org/freedesktop/NetworkManager/AccessPoint/foo",
            AccessPoint {
                ssid: String::from("foo"),
            },
        )
        .await?;
    server
        .at("/org/freedesktop/NetworkManager/IP4Config/1", ip4)
        .await?;

    conn.request_name("org.freedesktop.NetworkManager").await?;

    Ok(conn)
}

struct Ip4Config;

/// see: [IP4Config](https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.IP4Config.html)
#[interface(name = "org.freedesktop.NetworkManager.IP4Config")]
impl Ip4Config {
    #[zbus(property)]
    fn address_data(&self) -> Vec<HashMap<String, String>> {
        vec![HashMap::from_iter([
            (String::from("address"), String::from("1.2.3.4")),
            (String::from("prefix"), String::from("32")),
        ])]
    }
}
