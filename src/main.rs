#![deny(clippy::all, clippy::pedantic, unsafe_code)]
#![allow(clippy::unused_self)]

use std::collections::HashMap;

use clap::Parser;
use network_manager::NetworkManager;
use zbus::{
    conn::Builder,
    interface,
    zvariant::{ObjectPath, OwnedObjectPath},
};

mod enums;
mod network_manager;

use enums::{NMActivationStateFlags, NMActiveConnectionState, NMDeviceState, NMDeviceType};

struct ActiveConnection;

/// see: [NetworkManager.Connection.Active](https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Connection.Active.html)
#[interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl ActiveConnection {
    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        vec![
            ObjectPath::try_from("/org/freedesktop/NetworkManager/Devices/1")
                .expect("should parse into D-Bus object path")
                .into(),
        ]
    }

    #[zbus(property)]
    fn id(&self) -> String {
        String::from("1")
    }

    #[zbus(property)]
    fn ip4_config(&self) -> OwnedObjectPath {
        ObjectPath::try_from("/org/freedesktop/NetworkManager/IP4Config/1")
            .expect("should parse into D-Bus object path")
            .into()
    }

    #[zbus(property)]
    fn state(&self) -> u32 {
        NMActiveConnectionState::Activated as u32
    }

    #[zbus(property)]
    fn state_flags(&self) -> u32 {
        NMActivationStateFlags::None as u32
    }

    #[zbus(property)]
    fn vpn(&self) -> bool {
        false
    }
}

struct Device;

/// see: [Device](https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.html)
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

#[derive(Clone, Debug, Parser, PartialEq, Eq)]
#[command(about, version)]
struct Args;

#[tokio::main]
async fn main() -> Result<(), zbus::Error> {
    let ac = ActiveConnection;
    let d = Device;
    let dw = DeviceWired;
    let ip4 = Ip4Config;
    let nm = NetworkManager;

    let _conn = Builder::system()?
        .name("org.freedesktop.NetworkManager")?
        .serve_at("/org/freedesktop/NetworkManager", nm)?
        .serve_at("/org/freedesktop/NetworkManager/ActiveConnection/1", ac)?
        .serve_at("/org/freedesktop/NetworkManager/Devices/1", d)?
        .serve_at("/org/freedesktop/NetworkManager/Devices/eth0", dw)?
        .serve_at("/org/freedesktop/NetworkManager/IP4Config/1", ip4)?
        .build()
        .await?;

    Ok(())
}
