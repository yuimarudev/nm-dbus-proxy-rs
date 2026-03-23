use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;

use tokio::fs;
use zbus::{
    Connection, ObjectServer,
    fdo::{self, Properties},
    interface,
    names::InterfaceName,
    object_server::SignalEmitter,
    zvariant::{OwnedObjectPath, Value},
};

use crate::{
    iwd::State as IwdState,
    network_manager::settings_connection::{ConnectionSettings, SettingsConnection},
    persistence,
    runtime::{ConnectionOrigin, ConnectionRecord, Runtime},
    sync_backends,
    systemd_networkd::Manager,
};

#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub can_modify: bool,
    pub hostname: String,
    pub runtime: Runtime,
    pub version_id: u64,
}

#[interface(name = "org.freedesktop.NetworkManager.Settings")]
impl Settings {
    #[zbus(signal, name = "NewConnection")]
    pub(crate) async fn emit_new_connection(
        emitter: &SignalEmitter<'_>,
        connection: OwnedObjectPath,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "ConnectionRemoved")]
    pub(crate) async fn emit_connection_removed(
        emitter: &SignalEmitter<'_>,
        connection: OwnedObjectPath,
    ) -> zbus::Result<()>;

    async fn add_connection(
        &self,
        connection: ConnectionSettings,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<OwnedObjectPath> {
        self.add_connection_inner(connection, false, server, bus)
            .await
    }

    async fn add_connection_unsaved(
        &self,
        connection: ConnectionSettings,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<OwnedObjectPath> {
        self.add_connection_inner(connection, true, server, bus)
            .await
    }

    async fn add_connection2(
        &self,
        connection: ConnectionSettings,
        flags: u32,
        _args: HashMap<String, zbus::zvariant::OwnedValue>,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<(OwnedObjectPath, HashMap<String, zbus::zvariant::OwnedValue>)> {
        let flags_to_disk = 0x1_u32;
        let flags_in_memory = 0x2_u32;
        let flags_block_autoconnect = 0x20_u32;
        let allowed_flags = flags_to_disk | flags_in_memory | flags_block_autoconnect;
        if flags & !allowed_flags != 0 {
            return Err(fdo::Error::InvalidArgs(String::from(
                "unknown AddConnection2 flags",
            )));
        }
        let persist = match (flags & flags_to_disk != 0, flags & flags_in_memory != 0) {
            (true, false) => true,
            (false, true) => false,
            _ => {
                return Err(fdo::Error::InvalidArgs(String::from(
                    "AddConnection2 requires exactly one of to-disk or in-memory",
                )));
            }
        };
        let mut connection = connection;
        if flags & flags_block_autoconnect != 0 {
            connection
                .entry(String::from("connection"))
                .or_default()
                .insert(
                    String::from("autoconnect"),
                    zbus::zvariant::OwnedValue::from(false),
                );
        }
        let path = self
            .add_connection_inner(connection, !persist, server, bus)
            .await?;
        Ok((path, HashMap::new()))
    }

    async fn load_connections(
        &self,
        filenames: Vec<String>,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(connection)] bus: &Connection,
    ) -> (bool, Vec<String>) {
        self.import_connections(filenames, server, bus).await
    }

    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        self.runtime
            .connections()
            .into_iter()
            .map(|connection| connection.path)
            .collect()
    }

    async fn reload_connections(&self, #[zbus(connection)] bus: &Connection) -> bool {
        let Ok(files) = persistence::discover_connection_files().await else {
            return false;
        };
        let server = bus.object_server();
        let imported = self
            .import_connections(
                files
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>(),
                server,
                bus,
            )
            .await;
        if !imported.0 {
            return false;
        }
        self.prune_missing_user_connections(&files, server, bus)
            .await;
        let Ok(builder) = zbus::conn::Builder::system() else {
            return false;
        };
        let Ok(system_bus) = builder.build().await else {
            return false;
        };
        let Ok(manager) = Manager::request(&system_bus).await else {
            return false;
        };
        let wireless = IwdState::request(&system_bus).await.unwrap_or_default();
        sync_backends(bus, &self.runtime, manager, wireless)
            .await
            .is_ok()
    }

    async fn save_hostname(
        &self,
        hostname: &str,
        #[zbus(connection)] bus: &Connection,
    ) -> fdo::Result<()> {
        fs::write(hostname_path(), format!("{hostname}\n"))
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        self.runtime.set_hostname(hostname.to_string());
        emit_settings_changed(bus, &self.runtime, Some(hostname.to_string()))
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        Ok(())
    }

    fn get_connection_by_uuid(&self, uuid: &str) -> fdo::Result<OwnedObjectPath> {
        self.runtime
            .connection_by_uuid(uuid)
            .map(|connection| connection.path)
            .ok_or_else(|| fdo::Error::Failed(format!("No connection for UUID '{uuid}'")))
    }

    #[zbus(property)]
    fn can_modify(&self) -> bool {
        self.can_modify || current_can_modify()
    }

    #[zbus(property)]
    fn connections(&self) -> Vec<OwnedObjectPath> {
        self.list_connections()
    }

    #[zbus(property)]
    fn hostname(&self) -> String {
        let runtime_hostname = self.runtime.hostname();
        if runtime_hostname.is_empty() {
            self.hostname.clone()
        } else {
            runtime_hostname
        }
    }

    #[zbus(property)]
    fn version_id(&self) -> u64 {
        self.runtime.version_id().max(self.version_id)
    }
}

fn hostname_path() -> PathBuf {
    crate::config::current().hostname_path
}

fn current_can_modify() -> bool {
    [
        hostname_path(),
        crate::config::current().iwd_state_dir,
        crate::config::current().network_dir,
    ]
    .into_iter()
    .all(|path| path_writable(&path))
}

fn path_writable(path: &PathBuf) -> bool {
    let mut current = if path.exists() {
        path.clone()
    } else {
        path.parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| path.clone())
    };

    loop {
        match std::fs::metadata(&current) {
            Ok(metadata) => return !metadata.permissions().readonly(),
            Err(_) => {
                let Some(parent) = current.parent() else {
                    return false;
                };
                current = parent.to_path_buf();
            }
        }
    }
}

impl Settings {
    async fn add_connection_inner(
        &self,
        connection: ConnectionSettings,
        unsaved: bool,
        server: &ObjectServer,
        bus: &Connection,
    ) -> fdo::Result<OwnedObjectPath> {
        let uuid = connection
            .get("connection")
            .and_then(|settings| settings.get("uuid"))
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| String::try_from(value).ok())
            .ok_or_else(|| fdo::Error::InvalidArgs(String::from("missing connection.uuid")))?;
        let connection_type = connection
            .get("connection")
            .and_then(|settings| settings.get("type"))
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| String::try_from(value).ok())
            .ok_or_else(|| fdo::Error::InvalidArgs(String::from("missing connection.type")))?;
        let path = self.runtime.next_connection_path();
        let record = ConnectionRecord {
            connection_type,
            filename: String::new(),
            flags: 0,
            origin: ConnectionOrigin::User,
            path: path.clone(),
            settings: connection,
            unsaved,
            uuid,
        };
        let mut record = record;
        if !unsaved {
            persistence::persist_connection(&mut record)
                .await
                .map_err(fdo::Error::Failed)?;
        }
        self.runtime.add_connection(record);

        server
            .at(
                path.as_str(),
                SettingsConnection {
                    path: path.clone(),
                    runtime: self.runtime.clone(),
                },
            )
            .await
            .map_err(fdo::Error::from)?;
        crate::emit_object_added(
            bus,
            &path,
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await
        .map_err(|error| fdo::Error::Failed(error.to_string()))?;

        emit_settings_changed(bus, &self.runtime, None)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;

        Ok(path)
    }

    async fn import_connections(
        &self,
        filenames: Vec<String>,
        server: &ObjectServer,
        bus: &Connection,
    ) -> (bool, Vec<String>) {
        let mut failures = Vec::new();

        for filename in filenames {
            let path = PathBuf::from(&filename);
            match persistence::load_connection_from_path(&path).await {
                Ok(mut connection) => {
                    let existing = self.runtime.connection_by_uuid(&connection.uuid);
                    let path = existing
                        .as_ref()
                        .map(|record| record.path.clone())
                        .unwrap_or_else(|| self.runtime.next_connection_path());
                    connection.path = path.clone();
                    if existing.is_some() {
                        let _ = self.runtime.update_connection(&path, |record| {
                            *record = connection.clone();
                        });
                    } else {
                        self.runtime.add_connection(connection);
                        let _ = server
                            .at(
                                path.as_str(),
                                SettingsConnection {
                                    path: path.clone(),
                                    runtime: self.runtime.clone(),
                                },
                            )
                            .await;
                        let _ = crate::emit_object_added(
                            bus,
                            &path,
                            "org.freedesktop.NetworkManager.Settings.Connection",
                        )
                        .await;
                        let _ = emit_new_connection_signal(bus, &path).await;
                    }
                }
                Err(_) => failures.push(filename),
            }
        }

        let _ = emit_settings_changed(bus, &self.runtime, None).await;
        (failures.is_empty(), failures)
    }

    async fn prune_missing_user_connections(
        &self,
        files: &[PathBuf],
        server: &ObjectServer,
        bus: &Connection,
    ) {
        let known = files
            .iter()
            .map(|path| path.display().to_string())
            .collect::<std::collections::HashSet<_>>();
        let stale = self
            .runtime
            .connections()
            .into_iter()
            .filter(|connection| {
                connection.origin == ConnectionOrigin::User
                    && !connection.unsaved
                    && !connection.filename.is_empty()
                    && !known.contains(&connection.filename)
            })
            .map(|connection| connection.path)
            .collect::<Vec<_>>();
        for path in stale {
            self.runtime.remove_connection(&path);
            let _ = server.remove::<SettingsConnection, _>(path.as_str()).await;
            let _ = crate::emit_object_removed(
                bus,
                &path,
                "org.freedesktop.NetworkManager.Settings.Connection",
            )
            .await;
            let _ = emit_connection_removed_signal(bus, &path).await;
        }
    }
}

pub(crate) async fn emit_settings_changed(
    bus: &Connection,
    runtime: &Runtime,
    hostname: Option<String>,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(bus, "/org/freedesktop/NetworkManager/Settings")?;
    let mut changed = HashMap::from([
        (
            "Connections",
            Value::from(
                runtime
                    .connections()
                    .into_iter()
                    .map(|connection| connection.path)
                    .collect::<Vec<_>>(),
            ),
        ),
        ("VersionId", Value::from(runtime.version_id())),
    ]);
    if let Some(hostname) = hostname {
        changed.insert("Hostname", Value::from(hostname));
    }
    Properties::properties_changed(
        &emitter,
        InterfaceName::try_from("org.freedesktop.NetworkManager.Settings")
            .expect("settings interface name should be valid"),
        changed,
        Cow::Borrowed(&[]),
    )
    .await
}

pub(crate) async fn emit_new_connection_signal(
    bus: &Connection,
    path: &OwnedObjectPath,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(bus, "/org/freedesktop/NetworkManager/Settings")?;
    Settings::emit_new_connection(&emitter, path.clone()).await
}

pub(crate) async fn emit_connection_removed_signal(
    bus: &Connection,
    path: &OwnedObjectPath,
) -> zbus::Result<()> {
    let emitter = SignalEmitter::new(bus, "/org/freedesktop/NetworkManager/Settings")?;
    Settings::emit_connection_removed(&emitter, path.clone()).await
}
