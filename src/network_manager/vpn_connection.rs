use zbus::{interface, object_server::SignalEmitter};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VpnConnection {
    pub banner: String,
    pub vpn_state: u32,
}

#[interface(name = "org.freedesktop.NetworkManager.VPN.Connection")]
impl VpnConnection {
    #[zbus(signal, name = "VpnStateChanged")]
    pub(crate) async fn emit_vpn_state_changed(
        emitter: &SignalEmitter<'_>,
        state: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(property)]
    fn vpn_state(&self) -> u32 {
        self.vpn_state
    }

    #[zbus(property)]
    fn banner(&self) -> String {
        self.banner.clone()
    }
}
