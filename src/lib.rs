use std::collections::HashMap;

use network_manager::{NetworkManager, active_connection::ActiveConnection};
use zbus::{
    Address, Connection,
    conn::Builder,
    interface,
    zvariant::{ObjectPath, OwnedObjectPath},
};

mod enums;
mod network_manager;

use enums::{NMDeviceState, NMDeviceType};
use zbus_systemd::network1::ManagerProxy;

pub async fn start_service(address: Option<Address>) -> Result<Connection, zbus::Error> {
    let system_bus = Builder::system()?.build().await?;
    let manager = ManagerProxy::new(&system_bus).await?;

    let links = manager.list_links().await?;

    let d = Device;
    let dw = DeviceWired;
    let ip4 = Ip4Config;
    let nm = NetworkManager {
        active_connections: links
            .iter()
            .map(|(_id, id, _path)| {
                OwnedObjectPath::from(
                    ObjectPath::try_from(format!(
                        "/org/freedesktop/NetworkManager/ActiveConnections/{}",
                        id
                    ))
                    .expect("should parse object path"),
                )
            })
            .collect(),
    };

    let service_bus = if let Some(some) = address {
        Builder::address(some)?
    } else {
        Builder::system()?
    };

    let conn = service_bus.build().await?;

    let server = conn.object_server();
    server.at("/org/freedesktop/NetworkManager", nm).await?;

    for (_i, id, _path) in links {
        eprintln!("{_i:?} {id:?} {_path:?}");
        let object_path = format!("/org/freedesktop/NetworkManager/ActiveConnections/{}", id);
        server
            .at(object_path.as_str(), ActiveConnection { id })
            .await?;
    }

    server
        .at("/org/freedesktop/NetworkManager/Devices/eth0", d)
        .await?;
    server
        .at("/org/freedesktop/NetworkManager/Devices/eth0", dw)
        .await?;
    server
        .at("/org/freedesktop/NetworkManager/IP4Config/1", ip4)
        .await?;

    conn.request_name("org.freedesktop.NetworkManager").await?;

    Ok(conn)
}

/// see: [Device](https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.html)
struct Device;

#[interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        NMDeviceType::Ethernet as u32
    }

    #[zbus(property)]
    fn path(&self) -> String {
        String::from("eth0")
    }

    #[zbus(property)]
    fn state(&self) -> u32 {
        NMDeviceState::Activated as u32
    }
}

struct DeviceWired;

/// see: [Device.Wired](https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.Wired.html)
#[interface(name = "org.freedesktop.NetworkManager.Device.Wired")]
impl DeviceWired {
    #[zbus(property)]
    fn hw_address(&self) -> String {
        String::from("01:23:45:67:89:AB")
    }

    #[zbus(property)]
    fn perm_hw_address(&self) -> String {
        String::from("01:23:45:67:89:AB")
    }

    #[zbus(property)]
    fn speed(&self) -> u32 {
        1000
    }
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
