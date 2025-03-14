use zbus::interface;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceWired;

/// see: [Device.Wired]( https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.Wired.html )
#[interface(name = "org.freedesktop.NetworkManager.Device.Wired")]
impl DeviceWired {
    #[deprecated]
    #[zbus(property)]
    fn carrier(&self) -> bool {
        // TODO
        true
    }

    #[deprecated]
    #[zbus(property)]
    fn hw_address(&self) -> String {
        // TODO
        String::from("01:23:45:67:89:AB")
    }

    #[zbus(property)]
    fn perm_hw_address(&self) -> String {
        // TODO
        String::from("01:23:45:67:89:AB")
    }

    #[zbus(property)]
    fn speed(&self) -> u32 {
        // TODO
        1000
    }

    #[zbus(property)]
    fn s390_subchannels(&self) -> Vec<String> {
        // TODO
        vec![]
    }
}
