use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use zbus::{Connection, fdo, interface, object_server::SignalEmitter, zvariant::OwnedValue};

#[derive(Clone, Debug, Default)]
pub struct VpnPlugin;

fn state_slot() -> &'static Mutex<u32> {
    static SLOT: OnceLock<Mutex<u32>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(0))
}

#[interface(name = "org.freedesktop.NetworkManager.VPN.Plugin")]
impl VpnPlugin {
    #[zbus(signal, name = "StateChanged")]
    pub(crate) async fn emit_state_changed(
        emitter: &SignalEmitter<'_>,
        state: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "SecretsRequired")]
    pub(crate) async fn emit_secrets_required(
        emitter: &SignalEmitter<'_>,
        message: &str,
        hints: Vec<String>,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "Config")]
    pub(crate) async fn emit_config(
        emitter: &SignalEmitter<'_>,
        config: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "Ip4Config")]
    pub(crate) async fn emit_ip4_config(
        emitter: &SignalEmitter<'_>,
        config: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "Ip6Config")]
    pub(crate) async fn emit_ip6_config(
        emitter: &SignalEmitter<'_>,
        config: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "LoginBanner")]
    pub(crate) async fn emit_login_banner(
        emitter: &SignalEmitter<'_>,
        banner: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "Failure")]
    pub(crate) async fn emit_failure(
        emitter: &SignalEmitter<'_>,
        reason: u32,
    ) -> zbus::Result<()>;

    async fn connect(
        &self,
        _connection: HashMap<String, HashMap<String, OwnedValue>>,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        *state_slot().lock().expect("vpn plugin mutex poisoned") = 1;
        let emitter = SignalEmitter::new(bus, "/org/freedesktop/NetworkManager/VPN/Plugin")
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Self::emit_state_changed(&emitter, 1)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    async fn connect_interactive(
        &self,
        _connection: HashMap<String, HashMap<String, OwnedValue>>,
        _details: HashMap<String, OwnedValue>,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        *state_slot().lock().expect("vpn plugin mutex poisoned") = 1;
        let emitter = SignalEmitter::new(bus, "/org/freedesktop/NetworkManager/VPN/Plugin")
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Self::emit_state_changed(&emitter, 1)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    async fn need_secrets(
        &self,
        _settings: HashMap<String, HashMap<String, OwnedValue>>,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<String> {
        let emitter = SignalEmitter::new(bus, "/org/freedesktop/NetworkManager/VPN/Plugin")
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        let _ = Self::emit_secrets_required(&emitter, "", Vec::new()).await;
        Ok(String::new())
    }

    async fn disconnect(&self, #[zbus(connection)] bus: &Connection) -> fdo::Result<()> {
        *state_slot().lock().expect("vpn plugin mutex poisoned") = 0;
        let emitter = SignalEmitter::new(bus, "/org/freedesktop/NetworkManager/VPN/Plugin")
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Self::emit_state_changed(&emitter, 0)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    async fn set_config(
        &self,
        config: HashMap<String, OwnedValue>,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        let emitter = SignalEmitter::new(bus, "/org/freedesktop/NetworkManager/VPN/Plugin")
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        let _ = Self::emit_config(&emitter, config).await;
        Ok(())
    }

    async fn set_ip4_config(
        &self,
        config: HashMap<String, OwnedValue>,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        let emitter = SignalEmitter::new(bus, "/org/freedesktop/NetworkManager/VPN/Plugin")
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        let _ = Self::emit_ip4_config(&emitter, config).await;
        Ok(())
    }

    async fn set_ip6_config(
        &self,
        config: HashMap<String, OwnedValue>,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        let emitter = SignalEmitter::new(bus, "/org/freedesktop/NetworkManager/VPN/Plugin")
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        let _ = Self::emit_ip6_config(&emitter, config).await;
        Ok(())
    }

    async fn set_failure(&self, _reason: &str, #[zbus(connection)] bus: &Connection) -> fdo::Result<()> {
        *state_slot().lock().expect("vpn plugin mutex poisoned") = 0;
        let emitter = SignalEmitter::new(bus, "/org/freedesktop/NetworkManager/VPN/Plugin")
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        let _ = Self::emit_failure(&emitter, 0).await;
        let _ = Self::emit_state_changed(&emitter, 0).await;
        Ok(())
    }

    fn new_secrets(
        &self,
        _connection: HashMap<String, HashMap<String, OwnedValue>>,
    ) -> fdo::Result<()> {
        Ok(())
    }

    #[zbus(property)]
    fn state(&self) -> u32 {
        *state_slot().lock().expect("vpn plugin mutex poisoned")
    }
}
