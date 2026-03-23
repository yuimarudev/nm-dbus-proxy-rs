use zbus::interface;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WifiP2PPeer {
    pub flags: u32,
    pub hw_address: String,
    pub last_seen: i32,
    pub manufacturer: String,
    pub model: String,
    pub model_number: String,
    pub name: String,
    pub serial: String,
    pub strength: u8,
    pub wfd_ies: Vec<u8>,
}

#[interface(name = "org.freedesktop.NetworkManager.WifiP2PPeer")]
impl WifiP2PPeer {
    #[zbus(property)]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[zbus(property)]
    fn flags(&self) -> u32 {
        self.flags
    }

    #[zbus(property)]
    fn manufacturer(&self) -> String {
        self.manufacturer.clone()
    }

    #[zbus(property)]
    fn model(&self) -> String {
        self.model.clone()
    }

    #[zbus(property)]
    fn model_number(&self) -> String {
        self.model_number.clone()
    }

    #[zbus(property)]
    fn serial(&self) -> String {
        self.serial.clone()
    }

    #[zbus(property)]
    fn wfd_ies(&self) -> Vec<u8> {
        self.wfd_ies.clone()
    }

    #[zbus(property)]
    fn hw_address(&self) -> String {
        self.hw_address.clone()
    }

    #[zbus(property)]
    fn strength(&self) -> u8 {
        self.strength
    }

    #[zbus(property)]
    fn last_seen(&self) -> i32 {
        self.last_seen
    }
}
