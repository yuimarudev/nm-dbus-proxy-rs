use zbus::interface;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceLoopback;

#[interface(name = "org.freedesktop.NetworkManager.Device.Loopback")]
impl DeviceLoopback {}
