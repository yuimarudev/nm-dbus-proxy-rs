use std::borrow::Cow;
use std::collections::HashMap;

use zbus::{
    Connection, ObjectServer, Proxy,
    fdo::{self, Properties},
    interface,
    names::InterfaceName,
    object_server::SignalEmitter,
    zvariant::{OwnedObjectPath, Value},
};

use crate::{network_manager::settings::emit_settings_changed, persistence, runtime::Runtime};

pub type ConnectionSettings = HashMap<String, HashMap<String, zbus::zvariant::OwnedValue>>;

#[derive(Clone, Debug, Default)]
pub struct SettingsConnection {
    pub path: OwnedObjectPath,
    pub runtime: Runtime,
}

#[interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl SettingsConnection {
    #[zbus(signal, name = "Updated")]
    pub(crate) async fn emit_updated(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal, name = "Removed")]
    pub(crate) async fn emit_removed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    async fn get_secrets(
        &self,
        setting_name: &str,
        #[zbus(connection)] bus: &Connection,
    ) -> ConnectionSettings {
        if let Some(connection) = self.runtime.connection(&self.path) {
            if let Some(settings) = connection.settings.get(setting_name) {
                return HashMap::from([(String::from(setting_name), settings.clone())]);
            }

            return secrets_from_agent(bus, &self.runtime, &connection, setting_name)
                .await
                .unwrap_or_default();
        }

        HashMap::new()
    }

    fn get_settings(&self) -> ConnectionSettings {
        self.runtime
            .connection(&self.path)
            .map(|connection| connection.settings)
            .unwrap_or_default()
    }

    async fn clear_secrets(&self, #[zbus(connection)] bus: &Connection) -> fdo::Result<()> {
        let connection = self
            .runtime
            .connection(&self.path)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        let mut updated = connection.clone();
        updated.settings.remove("802-11-wireless-security");
        let _ = delete_secrets_from_agent(bus, &self.runtime, &updated).await;
        persist_prepared_connection(&connection, &mut updated)
            .await
            .map_err(fdo::Error::Failed)?;
        apply_runtime_connection(&self.runtime, &self.path, updated)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        emit_connection_changed(bus, &self.runtime, &self.path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_updated_signal(bus, &self.path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_settings_changed(bus, &self.runtime, None)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    async fn delete(
        &self,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        let connection = self
            .runtime
            .connection(&self.path)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        let _ = delete_secrets_from_agent(bus, &self.runtime, &connection).await;
        persistence::delete_persisted_connection(&connection)
            .await
            .map_err(fdo::Error::Failed)?;

        emit_removed_signal(bus, &self.path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        self.runtime
            .remove_connection(&self.path)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        server
            .remove::<Self, _>(self.path.as_str())
            .await
            .map_err(fdo::Error::from)?;
        crate::emit_object_removed(
            bus,
            &self.path,
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await
        .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        crate::network_manager::settings::emit_connection_removed_signal(bus, &self.path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_settings_changed(bus, &self.runtime, None)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    async fn save(&self, #[zbus(connection)] bus: &Connection) -> fdo::Result<()> {
        let connection = self
            .runtime
            .connection(&self.path)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        let mut updated = connection.clone();
        let _ = save_secrets_to_agent(bus, &self.runtime, &updated).await;
        updated.unsaved = false;
        persist_prepared_connection(&connection, &mut updated)
            .await
            .map_err(fdo::Error::Failed)?;
        apply_runtime_connection(&self.runtime, &self.path, updated)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        emit_connection_changed(bus, &self.runtime, &self.path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_updated_signal(bus, &self.path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_settings_changed(bus, &self.runtime, None)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    async fn update_unsaved(
        &self,
        properties: ConnectionSettings,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        let connection = self
            .runtime
            .connection(&self.path)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        let mut updated = connection.clone();
        updated.with_settings(properties);
        updated.unsaved = true;
        apply_runtime_connection(&self.runtime, &self.path, updated)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        emit_connection_changed(bus, &self.runtime, &self.path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_updated_signal(bus, &self.path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_settings_changed(bus, &self.runtime, None)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    async fn update(
        &self,
        properties: ConnectionSettings,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        let connection = self
            .runtime
            .connection(&self.path)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        let mut updated = connection.clone();
        updated.with_settings(properties);
        persist_prepared_connection(&connection, &mut updated)
            .await
            .map_err(fdo::Error::Failed)?;
        apply_runtime_connection(&self.runtime, &self.path, updated)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        emit_connection_changed(bus, &self.runtime, &self.path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_updated_signal(bus, &self.path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_settings_changed(bus, &self.runtime, None)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    async fn update2(
        &self,
        settings: ConnectionSettings,
        flags: u32,
        args: HashMap<String, zbus::zvariant::OwnedValue>,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        let allowed_flags = 0x1_u32 | 0x2_u32 | 0x4_u32 | 0x8_u32 | 0x10_u32 | 0x40_u32;
        if flags & !allowed_flags != 0 {
            return Err(fdo::Error::InvalidArgs(String::from("unknown Update2 flags")));
        }
        let connection = self
            .runtime
            .connection(&self.path)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        if let Some(expected) = args
            .get("version-id")
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| u64::try_from(value).ok())
        {
            if expected != self.runtime.version_id() {
                return Err(fdo::Error::Failed(String::from("version-id mismatch")));
            }
        }
        let mut updated = connection.clone();
        updated.with_settings(settings);
        let in_memory = flags & (0x2 | 0x4 | 0x8 | 0x10) != 0;
        let to_disk = flags & 0x1 != 0;
        if in_memory && to_disk {
            return Err(fdo::Error::InvalidArgs(String::from(
                "Update2 cannot combine to-disk and in-memory",
            )));
        }
        if in_memory {
            updated.unsaved = true;
        } else if to_disk {
            updated.unsaved = false;
        }
        persist_prepared_connection(&connection, &mut updated)
            .await
            .map_err(fdo::Error::Failed)?;
        apply_runtime_connection(&self.runtime, &self.path, updated)
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown connection")))?;
        emit_connection_changed(bus, &self.runtime, &self.path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_updated_signal(bus, &self.path)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        emit_settings_changed(bus, &self.runtime, None)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(HashMap::new())
    }

    #[zbus(property)]
    fn filename(&self) -> String {
        self.runtime
            .connection(&self.path)
            .map(|connection| connection.filename)
            .unwrap_or_default()
    }

    #[zbus(property)]
    fn flags(&self) -> u32 {
        self.runtime
            .connection(&self.path)
            .map(|connection| connection.flags)
            .unwrap_or_default()
    }

    #[zbus(property)]
    fn unsaved(&self) -> bool {
        self.runtime
            .connection(&self.path)
            .map(|connection| connection.unsaved)
            .unwrap_or_default()
    }
}

pub(crate) async fn emit_connection_changed(
    bus: &Connection,
    runtime: &Runtime,
    path: &OwnedObjectPath,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(bus, path.as_str())?;
    let Some(connection) = runtime.connection(path) else {
        return Ok(());
    };
    let changed = HashMap::from([
        ("Filename", Value::from(connection.filename)),
        ("Flags", Value::from(connection.flags)),
        ("Unsaved", Value::from(connection.unsaved)),
    ]);
    Properties::properties_changed(
        &emitter,
        InterfaceName::try_from("org.freedesktop.NetworkManager.Settings.Connection")
            .expect("settings connection interface name should be valid"),
        changed,
        Cow::Borrowed(&[]),
    )
    .await
}

pub(crate) async fn emit_updated_signal(
    bus: &Connection,
    path: &OwnedObjectPath,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(bus, path.as_str())?;
    SettingsConnection::emit_updated(&emitter).await
}

pub(crate) async fn emit_removed_signal(
    bus: &Connection,
    path: &OwnedObjectPath,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(bus, path.as_str())?;
    SettingsConnection::emit_removed(&emitter).await
}

pub(crate) async fn persist_runtime_connection(
    runtime: &Runtime,
    path: &OwnedObjectPath,
) -> Result<(), String> {
    let connection = runtime
        .connection(path)
        .ok_or_else(|| String::from("unknown connection"))?;
    let mut updated = connection.clone();
    persist_prepared_connection(&connection, &mut updated).await?;
    apply_runtime_connection(runtime, path, updated).ok_or_else(|| String::from("unknown connection"))?;
    Ok(())
}

fn apply_runtime_connection(
    runtime: &Runtime,
    path: &OwnedObjectPath,
    updated: crate::runtime::ConnectionRecord,
) -> Option<()> {
    runtime.update_connection(path, |current| {
        *current = updated;
    })
}

async fn persist_prepared_connection(
    existing: &crate::runtime::ConnectionRecord,
    updated: &mut crate::runtime::ConnectionRecord,
) -> Result<(), String> {
    if updated.unsaved {
        return Ok(());
    }

    let old_filename = existing.filename.clone();
    persistence::persist_connection(updated).await?;
    if !old_filename.is_empty() && old_filename != updated.filename {
        let stale = crate::runtime::ConnectionRecord {
            filename: old_filename,
            ..existing.clone()
        };
        persistence::delete_persisted_connection(&stale).await?;
    }
    Ok(())
}

async fn delete_secrets_from_agent(
    bus: &Connection,
    runtime: &Runtime,
    connection: &crate::runtime::ConnectionRecord,
) -> Option<()> {
    let agent = runtime.registered_agents().last().cloned()?;
    let proxy = secret_agent_proxy(bus, &agent).await?;
    proxy
        .call::<_, _, ()>(
            "DeleteSecrets",
            &(connection.settings.clone(), connection.path.clone()),
        )
        .await
        .ok()?;
    Some(())
}

async fn save_secrets_to_agent(
    bus: &Connection,
    runtime: &Runtime,
    connection: &crate::runtime::ConnectionRecord,
) -> Option<()> {
    let agent = runtime.registered_agents().last().cloned()?;
    let proxy = secret_agent_proxy(bus, &agent).await?;
    proxy
        .call::<_, _, ()>(
            "SaveSecrets",
            &(connection.settings.clone(), connection.path.clone()),
        )
        .await
        .ok()?;
    Some(())
}

async fn secret_agent_proxy<'a>(
    bus: &'a Connection,
    agent: &'a crate::runtime::RegisteredAgent,
) -> Option<Proxy<'a>> {
    Proxy::new(
        bus,
        agent.sender.as_str(),
        "/org/freedesktop/NetworkManager/SecretAgent",
        "org.freedesktop.NetworkManager.SecretAgent",
    )
    .await
    .ok()
}

async fn secrets_from_agent(
    bus: &Connection,
    runtime: &Runtime,
    connection: &crate::runtime::ConnectionRecord,
    setting_name: &str,
) -> Option<ConnectionSettings> {
    let agent = runtime.registered_agents().last().cloned()?;
    let proxy = secret_agent_proxy(bus, &agent).await?;
    proxy
        .call(
            "GetSecrets",
            &(
                connection.settings.clone(),
                connection.path.clone(),
                String::from(setting_name),
                Vec::<String>::new(),
                1u32,
            ),
        )
        .await
        .ok()
}
