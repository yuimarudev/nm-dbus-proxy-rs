use zbus::interface;

use crate::enums::NM80211Mode;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AccessPoint {
    pub ssid: String,
}

/// see: [Device.Wired]( https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.Wired.html )
#[interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl AccessPoint {
    #[zbus(property)]
    fn bandwidth(&self) -> u32 {
        // TODO
        1000
    }

    #[zbus(property)]
    fn flags(&self) -> u32 {
        // TODO
        0
    }

    #[zbus(property)]
    fn frequency(&self) -> u32 {
        // TODO
        0
    }

    #[zbus(property)]
    fn hw_address(&self) -> String {
        // TODO
        String::from("01:23:45:67:89:AB")
    }

    #[zbus(property)]
    fn last_seen(&self) -> i32 {
        // TODO
        -1
    }

    #[zbus(property)]
    fn max_bitrate(&self) -> u32 {
        // TODO
        1000
    }

    #[zbus(property)]
    fn mode(&self) -> u32 {
        // TODO
        NM80211Mode::Infra as u32
    }

    #[zbus(property)]
    fn rsn_flags(&self) -> u32 {
        // TODO
        0
    }

    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        self.ssid.as_bytes().to_vec()
    }

    #[zbus(property)]
    fn strength(&self) -> u8 {
        // TODO
        100
    }

    #[zbus(property)]
    fn wpa_flags(&self) -> u32 {
        // TODO
        0
    }
}
