use zbus::{
    interface,
    zvariant::{ObjectPath, OwnedObjectPath},
};

use crate::enums::NM80211Mode;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceWireless;

/// see: [Device.Wireless]( https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.Wireless.html )
#[interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl DeviceWireless {
    #[zbus(property)]
    fn access_points(&self) -> Vec<OwnedObjectPath> {
        // TODO
        vec![OwnedObjectPath::from(
            ObjectPath::try_from("/org/freedesktop/NetworkManager/AccessPoint/foo")
                .expect("should parse access points object path"),
        )]
    }

    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        // TODO
        OwnedObjectPath::from(
            ObjectPath::try_from("/org/freedesktop/NetworkManager/AccessPoint/foo")
                .expect("should parse access points object path"),
        )
    }

    #[zbus(property)]
    fn bitrate(&self) -> u32 {
        // TODO
        1000
    }

    #[deprecated]
    #[zbus(property)]
    fn hw_address(&self) -> String {
        // TODO
        String::from("01:23:45:67:89:AB")
    }

    #[zbus(property)]
    fn last_scan(&self) -> i64 {
        // TODO
        -1
    }

    #[zbus(property)]
    fn mode(&self) -> u32 {
        // TODO
        NM80211Mode::Infra as u32
    }

    #[zbus(property)]
    fn perm_hw_address(&self) -> String {
        // TODO
        String::from("01:23:45:67:89:AB")
    }

    #[zbus(property)]
    fn wireless_capabilities(&self) -> u32 {
        // TODO
        0
    }
}
