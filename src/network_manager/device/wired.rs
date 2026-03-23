// Modified by yuimarudev on 2026-03-23.
// This file contains changes from the original upstream work.
use zbus::interface;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceWired {
    pub carrier: bool,
    pub hw_address: String,
    pub perm_hw_address: String,
    pub s390_subchannels: Vec<String>,
    pub speed: u32,
}

/// see: [Device.Wired]( https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.Wired.html )
#[interface(name = "org.freedesktop.NetworkManager.Device.Wired")]
impl DeviceWired {
    #[deprecated]
    #[zbus(property)]
    fn carrier(&self) -> bool {
        self.carrier
    }

    #[deprecated]
    #[zbus(property)]
    fn hw_address(&self) -> String {
        self.hw_address.clone()
    }

    #[zbus(property)]
    fn perm_hw_address(&self) -> String {
        self.perm_hw_address.clone()
    }

    #[zbus(property)]
    fn speed(&self) -> u32 {
        self.speed
    }

    #[zbus(property)]
    fn s390_subchannels(&self) -> Vec<String> {
        self.s390_subchannels.clone()
    }
}
