// Modified by yuimarudev on 2026-03-23.
// This file contains changes from the original upstream work.
use std::{
    path::PathBuf,
    process::Stdio,
    sync::{Mutex, MutexGuard, OnceLock},
};

use anyhow::Result;
use nm_dbus_proxy::{
    Config, clear_config_override, set_config_override,
    iwd::{BasicServiceSet, Device as IwdDevice, KnownNetwork, Network, OrderedNetwork, State as IwdState, Station},
    start_service,
    start_service_with_runtime,
    sync_backends,
    systemd_networkd::{
        Manager,
        link::{Address as LinkAddress, Kind, Link, LinkDescription, Type},
    },
};
use tokio::{
    fs::write,
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use zbus::{
    Address as BusAddress,
    conn::Builder,
    fdo::ObjectManagerProxy,
    names::OwnedInterfaceName,
    zvariant::{OwnedObjectPath, OwnedValue},
};

const DAEMON_PATH: &str = "/usr/bin/dbus-daemon";

fn test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("test lock poisoned")
}

#[tokio::test]
async fn network_manager_service_exposes_settings_and_object_manager() -> Result<()> {
    let _guard = test_lock();
    let (address, _daemon) = spawn_test_bus().await?;
    let client_bus = Builder::address(BusAddress::try_from(address.as_str())?)?
        .build()
        .await?;

    let _service = start_service(
        Some(BusAddress::try_from(address.as_str()).expect("valid bus address")),
        fake_manager(),
        fake_iwd_state(),
    )
    .await
    .expect("service should attach to D-Bus bus");

    let object_manager = ObjectManagerProxy::builder(&client_bus)
        .destination("org.freedesktop.NetworkManager")?
        .path("/org/freedesktop")?
        .build()
        .await?;
    let managed_objects = object_manager.get_managed_objects().await?;
    assert!(managed_objects.contains_key(&owned_path("/org/freedesktop/NetworkManager")));
    assert!(managed_objects.contains_key(&owned_path("/org/freedesktop/NetworkManager/Settings")));
    assert!(managed_objects.contains_key(&owned_path("/org/freedesktop/NetworkManager/Settings/1")));
    assert!(managed_objects.contains_key(&owned_path("/org/freedesktop/NetworkManager/Devices/eth0")));

    let settings_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager/Settings",
        "org.freedesktop.NetworkManager.Settings",
    )
    .await?;
    let connections: Vec<OwnedObjectPath> = settings_proxy.call("ListConnections", &()).await?;
    assert_eq!(connections.len(), 2);

    let network_manager = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await?;
    let permissions: std::collections::HashMap<String, String> =
        network_manager.call("GetPermissions", &()).await?;
    assert_eq!(
        permissions.get("org.freedesktop.NetworkManager.network-control"),
        Some(&String::from("yes"))
    );

    let connection_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        connections[0].as_str(),
        "org.freedesktop.NetworkManager.Settings.Connection",
    )
    .await?;
    let settings: std::collections::HashMap<
        String,
        std::collections::HashMap<String, OwnedValue>,
    > = connection_proxy.call("GetSettings", &()).await?;
    assert!(settings.contains_key("connection"));

    Ok(())
}

#[tokio::test]
async fn mutating_apis_work_for_wifi_and_wired() -> Result<()> {
    let _guard = test_lock();
    let temp_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("mutating");
    let iwd_dir = temp_dir.join("iwd");
    let network_dir = temp_dir.join("networkd");
    let hostname_path = temp_dir.join("hostname");
    let resolv_conf_path = temp_dir.join("resolv.conf");
    let iwctl_log = temp_dir.join("iwctl.log");
    let networkctl_log = temp_dir.join("networkctl.log");
    tokio::fs::create_dir_all(&iwd_dir).await?;
    tokio::fs::create_dir_all(&network_dir).await?;
    write(&resolv_conf_path, "nameserver 9.9.9.9\n").await?;

    let iwctl_path = temp_dir.join("fake-iwctl.sh");
    let networkctl_path = temp_dir.join("fake-networkctl.sh");
    write(
        &iwctl_path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\nexit 0\n",
            iwctl_log.display()
        ),
    )
    .await?;
    write(
        &networkctl_path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\nexit 0\n",
            networkctl_log.display()
        ),
    )
    .await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&iwctl_path).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&iwctl_path, perms).await?;
        let mut perms = tokio::fs::metadata(&networkctl_path).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&networkctl_path, perms).await?;
    }

    set_config_override(Config {
        hostname_path,
        iwd_state_dir: iwd_dir.clone(),
        iwctl_bin: iwctl_path.display().to_string(),
        network_dir: network_dir.clone(),
        networkctl_bin: networkctl_path.display().to_string(),
        resolv_conf_path,
        ..Config::default()
    });

    let (address, _daemon) = spawn_test_bus().await?;
    let client_bus = Builder::address(BusAddress::try_from(address.as_str())?)?
        .build()
        .await?;
    let _service = start_service(
        Some(BusAddress::try_from(address.as_str()).expect("valid bus address")),
        fake_manager(),
        fake_iwd_state(),
    )
    .await?;

    let settings_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager/Settings",
        "org.freedesktop.NetworkManager.Settings",
    )
    .await?;
    let network_manager = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await?;
    let manager_properties = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.DBus.Properties",
    )
    .await?;

    let connections: Vec<OwnedObjectPath> = settings_proxy.call("ListConnections", &()).await?;
    let wifi_connection = connections
        .iter()
        .find(|path| path.as_str().ends_with("/1"))
        .expect("wifi connection should exist")
        .clone();
    let wired_connection = connections
        .iter()
        .find(|path| path.as_str().ends_with("/2"))
        .expect("wired connection should exist")
        .clone();

    let mut new_wifi = std::collections::HashMap::from([
        (
            String::from("connection"),
            std::collections::HashMap::from([
                (
                    String::from("uuid"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from(
                        "11111111-2222-3333-4444-555555555555",
                    )))
                    .expect("uuid fits"),
                ),
                (
                    String::from("id"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("new-wifi")))
                        .expect("id fits"),
                ),
                (
                    String::from("type"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from(
                        "802-11-wireless",
                    )))
                    .expect("type fits"),
                ),
                (
                    String::from("interface-name"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("wlan0")))
                        .expect("ifname fits"),
                ),
                (String::from("autoconnect"), OwnedValue::from(true)),
            ]),
        ),
        (
            String::from("802-11-wireless"),
            std::collections::HashMap::from([(
                String::from("ssid"),
                OwnedValue::try_from(zbus::zvariant::Value::from(b"new-wifi".to_vec()))
                    .expect("ssid fits"),
            )]),
        ),
        (
            String::from("802-11-wireless-security"),
            std::collections::HashMap::from([
                (
                    String::from("key-mgmt"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("wpa-psk")))
                        .expect("key-mgmt fits"),
                ),
                (
                    String::from("psk"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("secret-pass")))
                        .expect("psk fits"),
                ),
            ]),
        ),
    ]);
    eprintln!("step: add-and-activate");
    let (new_connection_path, new_active_path): (OwnedObjectPath, OwnedObjectPath) = network_manager
        .call(
            "AddAndActivateConnection",
            &(
                new_wifi.clone(),
                owned_path("/org/freedesktop/NetworkManager/Devices/wlan0"),
                owned_path("/"),
            ),
        )
        .await?;
    assert!(new_connection_path.as_str().contains("/Settings/"));
    assert_eq!(
        new_active_path,
        owned_path("/org/freedesktop/NetworkManager/ActiveConnections/wlan0")
    );

    let active_connections: Vec<OwnedObjectPath> = manager_properties
        .call("Get", &("org.freedesktop.NetworkManager", "ActiveConnections"))
        .await
        .and_then(|value: OwnedValue| Vec::<OwnedObjectPath>::try_from(value).map_err(Into::into))?;
    assert!(active_connections.contains(&new_active_path));

    let device_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager/Devices/wlan0",
        "org.freedesktop.NetworkManager.Device",
    )
    .await?;
    let device_props = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager/Devices/wlan0",
        "org.freedesktop.DBus.Properties",
    )
    .await?;
    eprintln!("step: get-applied");
    let (applied, _version): (
        std::collections::HashMap<String, std::collections::HashMap<String, OwnedValue>>,
        u64,
    ) = device_proxy.call("GetAppliedConnection", &(0u32,)).await?;
    assert!(applied.contains_key("connection"));

    eprintln!("step: set-autoconnect");
    let _: () = device_props
        .call(
            "Set",
            &(
                "org.freedesktop.NetworkManager.Device",
                "Autoconnect",
                OwnedValue::from(false),
            ),
        )
        .await?;
    let updated_connection = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        new_connection_path.as_str(),
        "org.freedesktop.NetworkManager.Settings.Connection",
    )
    .await?;
    let updated_settings: std::collections::HashMap<
        String,
        std::collections::HashMap<String, OwnedValue>,
    > = updated_connection.call("GetSettings", &()).await?;
    assert_eq!(
        bool::try_from(updated_settings["connection"]["autoconnect"].clone())?,
        false
    );

    eprintln!("step: disconnect");
    let _: () = device_proxy.call("Disconnect", &()).await?;
    let active_after_disconnect: Vec<OwnedObjectPath> = manager_properties
        .call("Get", &("org.freedesktop.NetworkManager", "ActiveConnections"))
        .await
        .and_then(|value: OwnedValue| Vec::<OwnedObjectPath>::try_from(value).map_err(Into::into))?;
    assert!(!active_after_disconnect.contains(&new_active_path));

    eprintln!("step: wired-activate");
    let wired_active_path: OwnedObjectPath = network_manager
        .call(
            "ActivateConnection",
            &(
                wired_connection,
                owned_path("/org/freedesktop/NetworkManager/Devices/eth0"),
                owned_path("/"),
            ),
        )
        .await?;
    assert_eq!(
        wired_active_path,
        owned_path("/org/freedesktop/NetworkManager/ActiveConnections/eth0")
    );
    eprintln!("step: wired-deactivate");
    let _: () = network_manager
        .call("DeactivateConnection", &(wired_active_path.clone(),))
        .await?;

    eprintln!("step: checkpoint-create");
    let checkpoint_path: OwnedObjectPath = network_manager
        .call(
            "CheckpointCreate",
            &(
                vec![owned_path("/org/freedesktop/NetworkManager/Devices/wlan0")],
                30_u32,
                0_u32,
            ),
        )
        .await?;
    let checkpoints: Vec<OwnedObjectPath> = manager_properties
        .call("Get", &("org.freedesktop.NetworkManager", "Checkpoints"))
        .await
        .and_then(|value: OwnedValue| Vec::<OwnedObjectPath>::try_from(value).map_err(Into::into))?;
    assert!(checkpoints.contains(&checkpoint_path));

    new_wifi
        .entry(String::from("connection"))
        .or_default()
        .insert(String::from("autoconnect"), OwnedValue::from(false));
    eprintln!("step: update");
    let _: () = updated_connection.call("Update", &(new_wifi,)).await?;
    eprintln!("step: checkpoint-rollback");
    let rollback_result: std::collections::HashMap<String, u32> = network_manager
        .call("CheckpointRollback", &(checkpoint_path,))
        .await?;
    assert_eq!(
        rollback_result.get("/org/freedesktop/NetworkManager/Devices/wlan0"),
        Some(&0_u32)
    );

    let iwctl_log_contents = tokio::fs::read_to_string(&iwctl_log).await?;
    let networkctl_log_contents = tokio::fs::read_to_string(&networkctl_log).await?;
    assert!(iwctl_log_contents.contains("station wlan0 connect new-wifi psk"));
    assert!(iwctl_log_contents.contains("station wlan0 disconnect"));
    assert!(networkctl_log_contents.contains("up eth0"));
    assert!(networkctl_log_contents.contains("down eth0"));

    clear_config_override();
    Ok(())
}

#[tokio::test]
async fn root_properties_and_connection_flags_are_mutable() -> Result<()> {
    let _guard = test_lock();
    let temp_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("root-flags");
    let iwd_dir = temp_dir.join("iwd");
    let network_dir = temp_dir.join("networkd");
    let hostname_path = temp_dir.join("hostname");
    let resolv_conf_path = temp_dir.join("resolv.conf");
    tokio::fs::create_dir_all(&iwd_dir).await?;
    tokio::fs::create_dir_all(&network_dir).await?;
    write(&resolv_conf_path, "nameserver 1.1.1.1\n").await?;

    set_config_override(Config {
        hostname_path,
        iwd_state_dir: iwd_dir,
        network_dir: network_dir.clone(),
        resolv_conf_path,
        ..Config::default()
    });

    let (address, _daemon) = spawn_test_bus().await?;
    let client_bus = Builder::address(BusAddress::try_from(address.as_str())?)?
        .build()
        .await?;
    let _service = start_service(
        Some(BusAddress::try_from(address.as_str()).expect("valid bus address")),
        fake_manager(),
        fake_iwd_state(),
    )
    .await?;

    let network_manager = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await?;
    let manager_props = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.DBus.Properties",
    )
    .await?;
    let settings_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager/Settings",
        "org.freedesktop.NetworkManager.Settings",
    )
    .await?;

    let (level, domains): (String, String) = network_manager.call("GetLogging", &()).await?;
    assert_eq!(level, "INFO");
    assert_eq!(domains, "DEFAULT");
    let _: () = network_manager.call("SetLogging", &("TRACE", "WIFI")).await?;
    let (level, domains): (String, String) = network_manager.call("GetLogging", &()).await?;
    assert_eq!(level, "TRACE");
    assert_eq!(domains, "WIFI");

    let _: () = manager_props
        .call(
            "Set",
            &(
                "org.freedesktop.NetworkManager",
                "WirelessEnabled",
                OwnedValue::from(false),
            ),
        )
        .await?;
    let wireless_enabled: OwnedValue = manager_props
        .call("Get", &("org.freedesktop.NetworkManager", "WirelessEnabled"))
        .await?;
    assert!(!bool::try_from(wireless_enabled)?);

    let _: () = manager_props
        .call(
            "Set",
            &(
                "org.freedesktop.NetworkManager",
                "ConnectivityCheckEnabled",
                OwnedValue::from(true),
            ),
        )
        .await?;
    let connectivity_check_enabled: OwnedValue = manager_props
        .call(
            "Get",
            &(
                "org.freedesktop.NetworkManager",
                "ConnectivityCheckEnabled",
            ),
        )
        .await?;
    assert!(bool::try_from(connectivity_check_enabled)?);

    let new_connection = std::collections::HashMap::from([
        (
            String::from("connection"),
            std::collections::HashMap::from([
                (
                    String::from("uuid"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from(
                        "99999999-8888-7777-6666-555555555555",
                    )))
                    .expect("uuid fits"),
                ),
                (
                    String::from("id"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("ephemeral")))
                        .expect("id fits"),
                ),
                (
                    String::from("type"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from(
                        "802-3-ethernet",
                    )))
                    .expect("type fits"),
                ),
                (
                    String::from("interface-name"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("eth0")))
                        .expect("ifname fits"),
                ),
                (String::from("autoconnect"), OwnedValue::from(true)),
            ]),
        ),
        (String::from("802-3-ethernet"), std::collections::HashMap::new()),
    ]);

    let invalid_add: zbus::Result<(OwnedObjectPath, std::collections::HashMap<String, OwnedValue>)> =
        settings_proxy.call(
            "AddConnection2",
            &(new_connection.clone(), 0_u32, std::collections::HashMap::<String, OwnedValue>::new()),
        )
        .await;
    assert!(invalid_add.is_err());

    let (connection_path, _result): (
        OwnedObjectPath,
        std::collections::HashMap<String, OwnedValue>,
    ) = settings_proxy
        .call(
            "AddConnection2",
            &(new_connection.clone(), 0x2_u32, std::collections::HashMap::<String, OwnedValue>::new()),
        )
        .await?;

    let connection_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        connection_path.as_str(),
        "org.freedesktop.NetworkManager.Settings.Connection",
    )
    .await?;
    let connection_props = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        connection_path.as_str(),
        "org.freedesktop.DBus.Properties",
    )
    .await?;

    let unsaved: OwnedValue = connection_props
        .call(
            "Get",
            &(
                "org.freedesktop.NetworkManager.Settings.Connection",
                "Unsaved",
            ),
        )
        .await?;
    assert!(bool::try_from(unsaved)?);

    let mut updated_connection = new_connection.clone();
    updated_connection
        .entry(String::from("connection"))
        .or_default()
        .insert(String::from("autoconnect"), OwnedValue::from(false));
    let _: () = connection_proxy
        .call("UpdateUnsaved", &(updated_connection.clone(),))
        .await?;
    let updated_settings: std::collections::HashMap<
        String,
        std::collections::HashMap<String, OwnedValue>,
    > = connection_proxy.call("GetSettings", &()).await?;
    assert!(!bool::try_from(
        updated_settings["connection"]["autoconnect"].clone()
    )?);

    let _: std::collections::HashMap<String, OwnedValue> = connection_proxy
        .call(
            "Update2",
            &(updated_connection, 0x1_u32, std::collections::HashMap::<String, OwnedValue>::new()),
        )
        .await?;
    let unsaved: OwnedValue = connection_props
        .call(
            "Get",
            &(
                "org.freedesktop.NetworkManager.Settings.Connection",
                "Unsaved",
            ),
        )
        .await?;
    assert!(!bool::try_from(unsaved)?);
    let filename: OwnedValue = connection_props
        .call(
            "Get",
            &(
                "org.freedesktop.NetworkManager.Settings.Connection",
                "Filename",
            ),
        )
        .await?;
    let filename = String::try_from(filename)?;
    assert!(filename.starts_with(network_dir.to_string_lossy().as_ref()));

    clear_config_override();
    Ok(())
}

#[tokio::test]
async fn software_devices_expose_specialized_interfaces() -> Result<()> {
    let _guard = test_lock();
    let (address, _daemon) = spawn_test_bus().await?;
    let client_bus = Builder::address(BusAddress::try_from(address.as_str())?)?
        .build()
        .await?;

    let _service = start_service(
        Some(BusAddress::try_from(address.as_str()).expect("valid bus address")),
        fake_manager_with_software_links(),
        fake_iwd_state(),
    )
    .await?;

    let object_manager = ObjectManagerProxy::builder(&client_bus)
        .destination("org.freedesktop.NetworkManager")?
        .path("/org/freedesktop")?
        .build()
        .await?;
    let managed_objects = object_manager.get_managed_objects().await?;

    let bridge = managed_objects
        .get(&owned_path("/org/freedesktop/NetworkManager/Devices/br0"))
        .expect("bridge device should exist");
    assert!(bridge.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.Device.Bridge"
    )?));
    assert!(bridge.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.Device.Statistics"
    )?));

    let wireguard = managed_objects
        .get(&owned_path("/org/freedesktop/NetworkManager/Devices/wg0"))
        .expect("wireguard device should exist");
    assert!(wireguard.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.Device.WireGuard"
    )?));
    assert!(wireguard.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.Device.Statistics"
    )?));

    Ok(())
}

#[tokio::test]
async fn backend_sync_adds_and_removes_devices() -> Result<()> {
    let _guard = test_lock();
    let (address, _daemon) = spawn_test_bus().await?;
    let client_bus = Builder::address(BusAddress::try_from(address.as_str())?)?
        .build()
        .await?;

    let (service_bus, runtime) = start_service_with_runtime(
        Some(BusAddress::try_from(address.as_str()).expect("valid bus address")),
        fake_manager(),
        fake_iwd_state(),
    )
    .await?;

    sync_backends(&service_bus, &runtime, fake_manager_with_software_links(), fake_iwd_state())
        .await?;

    let object_manager = ObjectManagerProxy::builder(&client_bus)
        .destination("org.freedesktop.NetworkManager")?
        .path("/org/freedesktop")?
        .build()
        .await?;
    let mut managed_objects = object_manager.get_managed_objects().await?;
    assert!(managed_objects.contains_key(&owned_path(
        "/org/freedesktop/NetworkManager/Devices/br0"
    )));
    assert!(managed_objects.contains_key(&owned_path(
        "/org/freedesktop/NetworkManager/Devices/wg0"
    )));

    let manager_props = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.DBus.Properties",
    )
    .await?;
    let devices: OwnedValue = manager_props
        .call("Get", &("org.freedesktop.NetworkManager", "Devices"))
        .await?;
    let devices = Vec::<OwnedObjectPath>::try_from(devices)?;
    assert!(devices.contains(&owned_path(
        "/org/freedesktop/NetworkManager/Devices/br0"
    )));

    sync_backends(&service_bus, &runtime, fake_manager(), fake_iwd_state()).await?;
    managed_objects = object_manager.get_managed_objects().await?;
    assert!(!managed_objects.contains_key(&owned_path(
        "/org/freedesktop/NetworkManager/Devices/br0"
    )));
    assert!(!managed_objects.contains_key(&owned_path(
        "/org/freedesktop/NetworkManager/Devices/wg0"
    )));
    let devices: OwnedValue = manager_props
        .call("Get", &("org.freedesktop.NetworkManager", "Devices"))
        .await?;
    let devices = Vec::<OwnedObjectPath>::try_from(devices)?;
    assert!(!devices.contains(&owned_path(
        "/org/freedesktop/NetworkManager/Devices/br0"
    )));

    Ok(())
}

#[tokio::test]
async fn checkpoint_rollback_restores_persisted_profiles() -> Result<()> {
    let _guard = test_lock();
    let temp_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("checkpoint-files");
    let iwd_dir = temp_dir.join("iwd");
    let network_dir = temp_dir.join("networkd");
    let hostname_path = temp_dir.join("hostname");
    let resolv_conf_path = temp_dir.join("resolv.conf");
    tokio::fs::create_dir_all(&iwd_dir).await?;
    tokio::fs::create_dir_all(&network_dir).await?;
    write(&resolv_conf_path, "nameserver 1.1.1.1\n").await?;

    set_config_override(Config {
        hostname_path,
        iwd_state_dir: iwd_dir.clone(),
        network_dir,
        resolv_conf_path,
        ..Config::default()
    });

    let (address, _daemon) = spawn_test_bus().await?;
    let client_bus = Builder::address(BusAddress::try_from(address.as_str())?)?
        .build()
        .await?;

    let _service = start_service(
        Some(BusAddress::try_from(address.as_str()).expect("valid bus address")),
        fake_manager(),
        fake_iwd_state(),
    )
    .await?;

    let settings_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager/Settings",
        "org.freedesktop.NetworkManager.Settings",
    )
    .await?;
    let network_manager = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await?;

    let wifi_connection = std::collections::HashMap::from([
        (
            String::from("connection"),
            std::collections::HashMap::from([
                (
                    String::from("uuid"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from(
                        "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
                    )))
                    .expect("uuid fits"),
                ),
                (
                    String::from("id"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("rollback-wifi")))
                        .expect("id fits"),
                ),
                (
                    String::from("type"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from(
                        "802-11-wireless",
                    )))
                    .expect("type fits"),
                ),
                (
                    String::from("interface-name"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("wlan0")))
                        .expect("ifname fits"),
                ),
                (String::from("autoconnect"), OwnedValue::from(true)),
            ]),
        ),
        (
            String::from("802-11-wireless"),
            std::collections::HashMap::from([(
                String::from("ssid"),
                OwnedValue::try_from(zbus::zvariant::Value::from(b"rollback-wifi".to_vec()))
                    .expect("ssid fits"),
            )]),
        ),
        (
            String::from("802-11-wireless-security"),
            std::collections::HashMap::from([
                (
                    String::from("key-mgmt"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("wpa-psk")))
                        .expect("key-mgmt fits"),
                ),
                (
                    String::from("psk"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("supersecret")))
                        .expect("psk fits"),
                ),
            ]),
        ),
    ]);
    let connection_path: OwnedObjectPath = settings_proxy
        .call("AddConnection", &(wifi_connection.clone(),))
        .await?;
    let checkpoint_path: OwnedObjectPath = network_manager
        .call("CheckpointCreate", &(Vec::<OwnedObjectPath>::new(), 30_u32, 0_u32))
        .await?;

    let connection_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        connection_path.as_str(),
        "org.freedesktop.NetworkManager.Settings.Connection",
    )
    .await?;
    let mut updated = wifi_connection;
    updated
        .entry(String::from("connection"))
        .or_default()
        .insert(String::from("autoconnect"), OwnedValue::from(false));
    let _: () = connection_proxy.call("Update", &(updated,)).await?;

    let profile_path = iwd_dir.join("rollback-wifi.psk");
    let contents = tokio::fs::read_to_string(&profile_path).await?;
    assert!(contents.contains("AutoConnect=false"));

    let _: std::collections::HashMap<String, u32> = network_manager
        .call("CheckpointRollback", &(checkpoint_path,))
        .await?;
    let contents = tokio::fs::read_to_string(&profile_path).await?;
    assert!(contents.contains("AutoConnect=true"));

    clear_config_override();
    Ok(())
}

#[tokio::test]
async fn checkpoint_timeout_rolls_back_automatically() -> Result<()> {
    let _guard = test_lock();
    let temp_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("checkpoint-timeout");
    let iwd_dir = temp_dir.join("iwd");
    let network_dir = temp_dir.join("networkd");
    let hostname_path = temp_dir.join("hostname");
    let resolv_conf_path = temp_dir.join("resolv.conf");
    tokio::fs::create_dir_all(&iwd_dir).await?;
    tokio::fs::create_dir_all(&network_dir).await?;
    write(&resolv_conf_path, "nameserver 1.1.1.1\n").await?;

    set_config_override(Config {
        hostname_path,
        iwd_state_dir: iwd_dir.clone(),
        network_dir,
        resolv_conf_path,
        ..Config::default()
    });

    let (address, _daemon) = spawn_test_bus().await?;
    let client_bus = Builder::address(BusAddress::try_from(address.as_str())?)?
        .build()
        .await?;

    let _service = start_service(
        Some(BusAddress::try_from(address.as_str()).expect("valid bus address")),
        fake_manager(),
        fake_iwd_state(),
    )
    .await?;

    let settings_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager/Settings",
        "org.freedesktop.NetworkManager.Settings",
    )
    .await?;
    let network_manager = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await?;

    let wifi_connection = std::collections::HashMap::from([
        (
            String::from("connection"),
            std::collections::HashMap::from([
                (
                    String::from("uuid"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from(
                        "bbbbbbbb-cccc-dddd-eeee-ffffffffffff",
                    )))
                    .expect("uuid fits"),
                ),
                (
                    String::from("id"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("timeout-wifi")))
                        .expect("id fits"),
                ),
                (
                    String::from("type"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from(
                        "802-11-wireless",
                    )))
                    .expect("type fits"),
                ),
                (
                    String::from("interface-name"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("wlan0")))
                        .expect("ifname fits"),
                ),
                (String::from("autoconnect"), OwnedValue::from(true)),
            ]),
        ),
        (
            String::from("802-11-wireless"),
            std::collections::HashMap::from([(
                String::from("ssid"),
                OwnedValue::try_from(zbus::zvariant::Value::from(b"timeout-wifi".to_vec()))
                    .expect("ssid fits"),
            )]),
        ),
        (
            String::from("802-11-wireless-security"),
            std::collections::HashMap::from([
                (
                    String::from("key-mgmt"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("wpa-psk")))
                        .expect("key-mgmt fits"),
                ),
                (
                    String::from("psk"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("timeoutsecret")))
                        .expect("psk fits"),
                ),
            ]),
        ),
    ]);
    let connection_path: OwnedObjectPath = settings_proxy
        .call("AddConnection", &(wifi_connection.clone(),))
        .await?;
    let checkpoint_path: OwnedObjectPath = network_manager
        .call("CheckpointCreate", &(Vec::<OwnedObjectPath>::new(), 1_u32, 0_u32))
        .await?;
    let checkpoint_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        checkpoint_path.as_str(),
        "org.freedesktop.NetworkManager.Checkpoint",
    )
    .await?;
    let created: i64 = checkpoint_proxy.get_property("Created").await?;
    assert!(created >= 0);

    let connection_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        connection_path.as_str(),
        "org.freedesktop.NetworkManager.Settings.Connection",
    )
    .await?;
    let mut updated = wifi_connection;
    updated
        .entry(String::from("connection"))
        .or_default()
        .insert(String::from("autoconnect"), OwnedValue::from(false));
    let _: () = connection_proxy.call("Update", &(updated,)).await?;
    let profile_path = iwd_dir.join("timeout-wifi.psk");
    let contents = tokio::fs::read_to_string(&profile_path).await?;
    assert!(contents.contains("AutoConnect=false"));

    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let contents = tokio::fs::read_to_string(&profile_path).await?;
    assert!(contents.contains("AutoConnect=true"));
    let object_manager = ObjectManagerProxy::builder(&client_bus)
        .destination("org.freedesktop.NetworkManager")?
        .path("/org/freedesktop")?
        .build()
        .await?;
    let managed_objects = object_manager.get_managed_objects().await?;
    assert!(!managed_objects.contains_key(&checkpoint_path));

    clear_config_override();
    Ok(())
}

#[tokio::test]
async fn p2p_devices_expose_wifi_p2p_interface() -> Result<()> {
    let _guard = test_lock();
    let (address, _daemon) = spawn_test_bus().await?;
    let client_bus = Builder::address(BusAddress::try_from(address.as_str())?)?
        .build()
        .await?;

    let _service = start_service(
        Some(BusAddress::try_from(address.as_str()).expect("valid bus address")),
        fake_manager_with_p2p_device(),
        fake_iwd_state_with_p2p_peer(),
    )
    .await?;

    let object_manager = ObjectManagerProxy::builder(&client_bus)
        .destination("org.freedesktop.NetworkManager")?
        .path("/org/freedesktop")?
        .build()
        .await?;
    let managed_objects = object_manager.get_managed_objects().await?;
    let p2p = managed_objects
        .get(&owned_path("/org/freedesktop/NetworkManager/Devices/p2p_2ddev_2dwlan0"))
        .expect("p2p device should exist");
    assert!(p2p.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.Device.WifiP2P"
    )?));
    let p2p_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager/Devices/p2p_2ddev_2dwlan0",
        "org.freedesktop.NetworkManager.Device.WifiP2P",
    )
    .await?;
    let peers: Vec<OwnedObjectPath> = p2p_proxy.get_property("Peers").await?;
    assert_eq!(peers.len(), 1);
    let peer = managed_objects
        .get(&peers[0])
        .expect("p2p peer should exist");
    assert!(peer.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.WifiP2PPeer"
    )?));

    Ok(())
}

#[tokio::test]
async fn vpn_active_connections_expose_vpn_connection_interface() -> Result<()> {
    let _guard = test_lock();
    let temp_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("vpn-active");
    let iwd_dir = temp_dir.join("iwd");
    let network_dir = temp_dir.join("networkd");
    let hostname_path = temp_dir.join("hostname");
    let resolv_conf_path = temp_dir.join("resolv.conf");
    let networkctl_log = temp_dir.join("networkctl.log");
    tokio::fs::create_dir_all(&iwd_dir).await?;
    tokio::fs::create_dir_all(&network_dir).await?;
    write(&resolv_conf_path, "nameserver 9.9.9.9\n").await?;

    let networkctl_path = temp_dir.join("fake-networkctl.sh");
    write(
        &networkctl_path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\nexit 0\n",
            networkctl_log.display()
        ),
    )
    .await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&networkctl_path).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&networkctl_path, perms).await?;
    }

    set_config_override(Config {
        hostname_path,
        iwd_state_dir: iwd_dir,
        network_dir,
        networkctl_bin: networkctl_path.display().to_string(),
        resolv_conf_path,
        ..Config::default()
    });

    let (address, _daemon) = spawn_test_bus().await?;
    let client_bus = Builder::address(BusAddress::try_from(address.as_str())?)?
        .build()
        .await?;

    let _service = start_service(
        Some(BusAddress::try_from(address.as_str()).expect("valid bus address")),
        fake_manager(),
        fake_iwd_state(),
    )
    .await?;

    let settings_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager/Settings",
        "org.freedesktop.NetworkManager.Settings",
    )
    .await?;
    let network_manager = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await?;
    let object_manager = ObjectManagerProxy::builder(&client_bus)
        .destination("org.freedesktop.NetworkManager")?
        .path("/org/freedesktop")?
        .build()
        .await?;

    let vpn_connection = std::collections::HashMap::from([
        (
            String::from("connection"),
            std::collections::HashMap::from([
                (
                    String::from("uuid"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from(
                        "12121212-3434-5656-7878-909090909090",
                    )))
                    .expect("uuid fits"),
                ),
                (
                    String::from("id"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("vpn-profile")))
                        .expect("id fits"),
                ),
                (
                    String::from("type"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("vpn")))
                        .expect("type fits"),
                ),
                (
                    String::from("interface-name"),
                    OwnedValue::try_from(zbus::zvariant::Value::from(String::from("eth0")))
                        .expect("ifname fits"),
                ),
                (String::from("autoconnect"), OwnedValue::from(true)),
            ]),
        ),
        (String::from("vpn"), std::collections::HashMap::new()),
    ]);
    let connection_path: OwnedObjectPath = settings_proxy
        .call("AddConnection", &(vpn_connection,))
        .await?;
    let active_path: OwnedObjectPath = network_manager
        .call(
            "ActivateConnection",
            &(
                connection_path,
                owned_path("/org/freedesktop/NetworkManager/Devices/eth0"),
                owned_path("/"),
            ),
        )
        .await?;

    let managed_objects = object_manager.get_managed_objects().await?;
    let active = managed_objects
        .get(&active_path)
        .expect("vpn active connection should exist");
    assert!(active.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.VPN.Connection"
    )?));

    clear_config_override();
    Ok(())
}

#[tokio::test]
async fn vpn_plugin_object_is_exposed() -> Result<()> {
    let _guard = test_lock();
    let (address, _daemon) = spawn_test_bus().await?;
    let client_bus = Builder::address(BusAddress::try_from(address.as_str())?)?
        .build()
        .await?;

    let _service = start_service(
        Some(BusAddress::try_from(address.as_str()).expect("valid bus address")),
        fake_manager(),
        fake_iwd_state(),
    )
    .await?;

    let object_manager = ObjectManagerProxy::builder(&client_bus)
        .destination("org.freedesktop.NetworkManager")?
        .path("/org/freedesktop")?
        .build()
        .await?;
    let managed_objects = object_manager.get_managed_objects().await?;
    let plugin = managed_objects
        .get(&owned_path("/org/freedesktop/NetworkManager/VPN/Plugin"))
        .expect("vpn plugin object should exist");
    assert!(plugin.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.VPN.Plugin"
    )?));
    let plugin_proxy = zbus::Proxy::new(
        &client_bus,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager/VPN/Plugin",
        "org.freedesktop.NetworkManager.VPN.Plugin",
    )
    .await?;
    let state: u32 = plugin_proxy.get_property("State").await?;
    assert_eq!(state, 0);
    let _: () = plugin_proxy
        .call("Connect", &(std::collections::HashMap::<String, std::collections::HashMap<String, OwnedValue>>::new(),))
        .await?;
    let _: () = plugin_proxy.call("Disconnect", &()).await?;
    let state: u32 = plugin_proxy.get_property("State").await?;
    assert!(state <= 1);

    Ok(())
}

#[tokio::test]
async fn ppp_devices_expose_ppp_interfaces() -> Result<()> {
    let _guard = test_lock();
    let (address, _daemon) = spawn_test_bus().await?;
    let client_bus = Builder::address(BusAddress::try_from(address.as_str())?)?
        .build()
        .await?;

    let _service = start_service(
        Some(BusAddress::try_from(address.as_str()).expect("valid bus address")),
        fake_manager_with_ppp_device(),
        fake_iwd_state(),
    )
    .await?;

    let object_manager = ObjectManagerProxy::builder(&client_bus)
        .destination("org.freedesktop.NetworkManager")?
        .path("/org/freedesktop")?
        .build()
        .await?;
    let managed_objects = object_manager.get_managed_objects().await?;
    let ppp = managed_objects
        .get(&owned_path("/org/freedesktop/NetworkManager/Devices/ppp0"))
        .expect("ppp device should exist");
    assert!(ppp.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.Device.Ppp"
    )?));
    assert!(ppp.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.PPP"
    )?));

    Ok(())
}

#[tokio::test]
async fn typed_devices_expose_matching_specialized_interfaces() -> Result<()> {
    let _guard = test_lock();
    let (address, _daemon) = spawn_test_bus().await?;
    let client_bus = Builder::address(BusAddress::try_from(address.as_str())?)?
        .build()
        .await?;

    let _service = start_service(
        Some(BusAddress::try_from(address.as_str()).expect("valid bus address")),
        fake_manager_with_typed_special_devices(),
        fake_iwd_state(),
    )
    .await?;

    let object_manager = ObjectManagerProxy::builder(&client_bus)
        .destination("org.freedesktop.NetworkManager")?
        .path("/org/freedesktop")?
        .build()
        .await?;
    let managed_objects = object_manager.get_managed_objects().await?;

    let bluetooth = managed_objects
        .get(&owned_path("/org/freedesktop/NetworkManager/Devices/bt0"))
        .expect("bluetooth device should exist");
    assert!(bluetooth.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.Device.Bluetooth"
    )?));

    let infiniband = managed_objects
        .get(&owned_path("/org/freedesktop/NetworkManager/Devices/ib0"))
        .expect("infiniband device should exist");
    assert!(infiniband.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.Device.Infiniband"
    )?));

    let wpan = managed_objects
        .get(&owned_path("/org/freedesktop/NetworkManager/Devices/wpan0"))
        .expect("wpan device should exist");
    assert!(wpan.contains_key(&OwnedInterfaceName::try_from(
        "org.freedesktop.NetworkManager.Device.Wpan"
    )?));

    Ok(())
}

fn fake_iwd_state() -> IwdState {
    let known_network_path = owned_path("/net/connman/iwd/746573742d77696669_psk");
    let station_path = owned_path("/net/connman/iwd/0/6");
    let network_path = owned_path("/net/connman/iwd/0/6/746573742d77696669_psk");
    let bss_path = owned_path("/net/connman/iwd/0/6/746573742d77696669_psk/001122334455");

    IwdState {
        basic_service_sets: vec![BasicServiceSet {
            path: bss_path.clone(),
            address: String::from("00:11:22:33:44:55"),
        }],
        devices: vec![IwdDevice {
            path: station_path.clone(),
            name: String::from("wlan0"),
            address: String::from("02:00:00:00:00:02"),
            powered: true,
            adapter: owned_path("/net/connman/iwd/0"),
            mode: String::from("station"),
        }],
        known_networks: vec![KnownNetwork {
            path: known_network_path.clone(),
            name: String::from("test-wifi"),
            kind: String::from("psk"),
            hidden: false,
            auto_connect: true,
            last_connected_time: String::from("2026-03-22T09:00:00Z"),
        }],
        networks: vec![Network {
            path: network_path.clone(),
            name: String::from("test-wifi"),
            connected: false,
            device: station_path.clone(),
            kind: String::from("psk"),
            known_network: known_network_path,
            extended_service_set: vec![bss_path],
        }],
        stations: vec![Station {
            path: station_path,
            scanning: false,
            state: String::from("disconnected"),
            connected_network: None,
            ordered_networks: vec![OrderedNetwork {
                path: network_path,
                signal: -5000,
            }],
        }],
    }
}

fn fake_manager() -> Manager {
    Manager {
        links: vec![
            Link {
                address_state: String::from("routable"),
                administrative_state: String::from("configured"),
                bit_rates: (1_000_000_000, 1_000_000_000),
                carrier_state: String::from("carrier"),
                description: LinkDescription {
                    addresses: vec![LinkAddress {
                        address_string: String::from("192.0.2.10"),
                        config_state: String::from("configured"),
                        family: 2,
                        prefix_length: 24,
                    }],
                    alternative_names: vec![],
                    driver: String::from("ipheth"),
                    hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
                    index: 4,
                    kind: Default::default(),
                    mtu: 1500,
                    name: String::from("eth0"),
                    permanent_hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
                    r#type: Type::Ether,
                    wireless_lan_interface_type: String::new(),
                },
                ipv4_address_state: String::from("routable"),
                ipv6_address_state: String::from("off"),
                online_state: String::from("online"),
                operational_state: String::from("routable"),
            },
            Link {
                address_state: String::from("off"),
                administrative_state: String::from("configured"),
                bit_rates: (0, 0),
                carrier_state: String::from("no-carrier"),
                description: LinkDescription {
                    addresses: vec![],
                    alternative_names: vec![],
                    driver: String::from("iwlwifi"),
                    hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x02],
                    index: 6,
                    kind: Default::default(),
                    mtu: 1500,
                    name: String::from("wlan0"),
                    permanent_hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x03],
                    r#type: Type::Wlan,
                    wireless_lan_interface_type: String::from("station"),
                },
                ipv4_address_state: String::from("off"),
                ipv6_address_state: String::from("off"),
                online_state: String::from("offline"),
                operational_state: String::from("no-carrier"),
            },
        ],
    }
}

fn fake_manager_connected() -> Manager {
    Manager {
        links: vec![
            Link {
                address_state: String::from("routable"),
                administrative_state: String::from("configured"),
                bit_rates: (1_000_000_000, 1_000_000_000),
                carrier_state: String::from("carrier"),
                description: LinkDescription {
                    addresses: vec![LinkAddress {
                        address_string: String::from("192.0.2.10"),
                        config_state: String::from("configured"),
                        family: 2,
                        prefix_length: 24,
                    }],
                    alternative_names: vec![],
                    driver: String::from("ipheth"),
                    hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
                    index: 4,
                    kind: Default::default(),
                    mtu: 1500,
                    name: String::from("eth0"),
                    permanent_hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
                    r#type: Type::Ether,
                    wireless_lan_interface_type: String::new(),
                },
                ipv4_address_state: String::from("routable"),
                ipv6_address_state: String::from("off"),
                online_state: String::from("online"),
                operational_state: String::from("routable"),
            },
            Link {
                address_state: String::from("routable"),
                administrative_state: String::from("configured"),
                bit_rates: (600_000_000, 600_000_000),
                carrier_state: String::from("carrier"),
                description: LinkDescription {
                    addresses: vec![LinkAddress {
                        address_string: String::from("198.51.100.10"),
                        config_state: String::from("configured"),
                        family: 2,
                        prefix_length: 24,
                    }],
                    alternative_names: vec![],
                    driver: String::from("iwlwifi"),
                    hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x02],
                    index: 6,
                    kind: Default::default(),
                    mtu: 1500,
                    name: String::from("wlan0"),
                    permanent_hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x03],
                    r#type: Type::Wlan,
                    wireless_lan_interface_type: String::from("station"),
                },
                ipv4_address_state: String::from("routable"),
                ipv6_address_state: String::from("off"),
                online_state: String::from("online"),
                operational_state: String::from("routable"),
            },
        ],
    }
}

fn fake_iwd_state_connected() -> IwdState {
    let known_network_path = owned_path("/net/connman/iwd/746573742d77696669_psk");
    let cafe_known_path = owned_path("/net/connman/iwd/636166652d6f70656e_open");
    let station_path = owned_path("/net/connman/iwd/0/6");
    let network_path = owned_path("/net/connman/iwd/0/6/746573742d77696669_psk");
    let cafe_network_path = owned_path("/net/connman/iwd/0/6/636166652d6f70656e_open");
    let bss_path = owned_path("/net/connman/iwd/0/6/746573742d77696669_psk/001122334455");
    let cafe_bss_path = owned_path("/net/connman/iwd/0/6/636166652d6f70656e_open/66778899aabb");

    IwdState {
        basic_service_sets: vec![
            BasicServiceSet {
                path: bss_path.clone(),
                address: String::from("00:11:22:33:44:55"),
            },
            BasicServiceSet {
                path: cafe_bss_path.clone(),
                address: String::from("66:77:88:99:aa:bb"),
            },
        ],
        devices: vec![IwdDevice {
            path: station_path.clone(),
            name: String::from("wlan0"),
            address: String::from("02:00:00:00:00:02"),
            powered: true,
            adapter: owned_path("/net/connman/iwd/0"),
            mode: String::from("station"),
        }],
        known_networks: vec![
            KnownNetwork {
                path: known_network_path.clone(),
                name: String::from("test-wifi"),
                kind: String::from("psk"),
                hidden: false,
                auto_connect: true,
                last_connected_time: String::from("2026-03-22T09:00:00Z"),
            },
            KnownNetwork {
                path: cafe_known_path.clone(),
                name: String::from("cafe-open"),
                kind: String::from("open"),
                hidden: false,
                auto_connect: false,
                last_connected_time: String::new(),
            },
        ],
        networks: vec![
            Network {
                path: network_path.clone(),
                name: String::from("test-wifi"),
                connected: true,
                device: station_path.clone(),
                kind: String::from("psk"),
                known_network: known_network_path,
                extended_service_set: vec![bss_path],
            },
            Network {
                path: cafe_network_path.clone(),
                name: String::from("cafe-open"),
                connected: false,
                device: station_path.clone(),
                kind: String::from("open"),
                known_network: cafe_known_path,
                extended_service_set: vec![cafe_bss_path],
            },
        ],
        stations: vec![Station {
            path: station_path,
            scanning: false,
            state: String::from("connected"),
            connected_network: Some(network_path.clone()),
            ordered_networks: vec![
                OrderedNetwork {
                    path: network_path,
                    signal: -4200,
                },
                OrderedNetwork {
                    path: cafe_network_path,
                    signal: -6200,
                },
            ],
        }],
    }
}

fn fake_manager_with_virtual() -> Manager {
    let mut manager = fake_manager();
    manager.links.push(Link {
        address_state: String::from("off"),
        administrative_state: String::from("configured"),
        bit_rates: (0, 0),
        carrier_state: String::from("off"),
        description: LinkDescription {
            addresses: vec![],
            alternative_names: vec![],
            driver: String::from("veth"),
            hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x04],
            index: 7,
            kind: Kind::Veth,
            mtu: 1500,
            name: String::from("veth0"),
            permanent_hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x04],
            r#type: Type::Ether,
            wireless_lan_interface_type: String::new(),
        },
        ipv4_address_state: String::from("off"),
        ipv6_address_state: String::from("off"),
        online_state: String::from("offline"),
        operational_state: String::from("off"),
    });
    manager
}

fn fake_manager_with_software_links() -> Manager {
    let mut manager = fake_manager();
    manager.links.push(Link {
        address_state: String::from("off"),
        administrative_state: String::from("configured"),
        bit_rates: (0, 0),
        carrier_state: String::from("off"),
        description: LinkDescription {
            addresses: vec![],
            alternative_names: vec![],
            driver: String::from("bridge"),
            hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x05],
            index: 8,
            kind: Kind::Bridge,
            mtu: 1500,
            name: String::from("br0"),
            permanent_hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x05],
            r#type: Type::Ether,
            wireless_lan_interface_type: String::new(),
        },
        ipv4_address_state: String::from("off"),
        ipv6_address_state: String::from("off"),
        online_state: String::from("offline"),
        operational_state: String::from("off"),
    });
    manager.links.push(Link {
        address_state: String::from("off"),
        administrative_state: String::from("configured"),
        bit_rates: (0, 0),
        carrier_state: String::from("off"),
        description: LinkDescription {
            addresses: vec![],
            alternative_names: vec![],
            driver: String::from("wireguard"),
            hardware_address: vec![],
            index: 9,
            kind: Kind::Wireguard,
            mtu: 1420,
            name: String::from("wg0"),
            permanent_hardware_address: vec![],
            r#type: Type::Ether,
            wireless_lan_interface_type: String::new(),
        },
        ipv4_address_state: String::from("off"),
        ipv6_address_state: String::from("off"),
        online_state: String::from("offline"),
        operational_state: String::from("off"),
    });
    manager
}

fn fake_manager_with_p2p_device() -> Manager {
    let mut manager = fake_manager();
    manager.links.push(Link {
        address_state: String::from("off"),
        administrative_state: String::from("configured"),
        bit_rates: (0, 0),
        carrier_state: String::from("off"),
        description: LinkDescription {
            addresses: vec![],
            alternative_names: vec![],
            driver: String::from("iwlwifi"),
            hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x06],
            index: 10,
            kind: Default::default(),
            mtu: 1500,
            name: String::from("p2p-dev-wlan0"),
            permanent_hardware_address: vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x06],
            r#type: Type::Wlan,
            wireless_lan_interface_type: String::from("p2p-device"),
        },
        ipv4_address_state: String::from("off"),
        ipv6_address_state: String::from("off"),
        online_state: String::from("offline"),
        operational_state: String::from("off"),
    });
    manager
}

fn fake_manager_with_ppp_device() -> Manager {
    let mut manager = fake_manager();
    manager.links.push(Link {
        address_state: String::from("routable"),
        administrative_state: String::from("configured"),
        bit_rates: (0, 0),
        carrier_state: String::from("carrier"),
        description: LinkDescription {
            addresses: vec![LinkAddress {
                address_string: String::from("203.0.113.10"),
                config_state: String::from("configured"),
                family: 2,
                prefix_length: 32,
            }],
            alternative_names: vec![],
            driver: String::from("ppp"),
            hardware_address: vec![],
            index: 11,
            kind: Default::default(),
            mtu: 1492,
            name: String::from("ppp0"),
            permanent_hardware_address: vec![],
            r#type: Type::None,
            wireless_lan_interface_type: String::new(),
        },
        ipv4_address_state: String::from("routable"),
        ipv6_address_state: String::from("off"),
        online_state: String::from("online"),
        operational_state: String::from("routable"),
    });
    manager
}

fn fake_manager_with_typed_special_devices() -> Manager {
    let mut manager = fake_manager();
    manager.links.push(Link {
        address_state: String::from("off"),
        administrative_state: String::from("configured"),
        bit_rates: (0, 0),
        carrier_state: String::from("off"),
        description: LinkDescription {
            addresses: vec![],
            alternative_names: vec![],
            driver: String::from("bluetooth"),
            hardware_address: vec![],
            index: 12,
            kind: Default::default(),
            mtu: 1500,
            name: String::from("bt0"),
            permanent_hardware_address: vec![],
            r#type: Type::Bluetooth,
            wireless_lan_interface_type: String::new(),
        },
        ipv4_address_state: String::from("off"),
        ipv6_address_state: String::from("off"),
        online_state: String::from("offline"),
        operational_state: String::from("off"),
    });
    manager.links.push(Link {
        address_state: String::from("off"),
        administrative_state: String::from("configured"),
        bit_rates: (0, 0),
        carrier_state: String::from("off"),
        description: LinkDescription {
            addresses: vec![],
            alternative_names: vec![],
            driver: String::from("infiniband"),
            hardware_address: vec![],
            index: 13,
            kind: Default::default(),
            mtu: 2044,
            name: String::from("ib0"),
            permanent_hardware_address: vec![],
            r#type: Type::Infiniband,
            wireless_lan_interface_type: String::new(),
        },
        ipv4_address_state: String::from("off"),
        ipv6_address_state: String::from("off"),
        online_state: String::from("offline"),
        operational_state: String::from("off"),
    });
    manager.links.push(Link {
        address_state: String::from("off"),
        administrative_state: String::from("configured"),
        bit_rates: (0, 0),
        carrier_state: String::from("off"),
        description: LinkDescription {
            addresses: vec![],
            alternative_names: vec![],
            driver: String::from("wpan"),
            hardware_address: vec![],
            index: 14,
            kind: Default::default(),
            mtu: 1280,
            name: String::from("wpan0"),
            permanent_hardware_address: vec![],
            r#type: Type::Wpan,
            wireless_lan_interface_type: String::new(),
        },
        ipv4_address_state: String::from("off"),
        ipv6_address_state: String::from("off"),
        online_state: String::from("offline"),
        operational_state: String::from("off"),
    });
    manager
}

fn fake_iwd_state_with_p2p_peer() -> IwdState {
    let known_network_path = owned_path("/net/connman/iwd/p2ppeer_open");
    let station_path = owned_path("/net/connman/iwd/0/10");
    let network_path = owned_path("/net/connman/iwd/0/10/p2ppeer_open");
    let bss_path = owned_path("/net/connman/iwd/0/10/p2ppeer_open/112233445566");

    IwdState {
        basic_service_sets: vec![BasicServiceSet {
            path: bss_path.clone(),
            address: String::from("11:22:33:44:55:66"),
        }],
        devices: vec![IwdDevice {
            path: station_path.clone(),
            name: String::from("p2p-dev-wlan0"),
            address: String::from("02:00:00:00:00:06"),
            powered: true,
            adapter: owned_path("/net/connman/iwd/0"),
            mode: String::from("station"),
        }],
        known_networks: vec![KnownNetwork {
            path: known_network_path.clone(),
            name: String::from("peer-one"),
            kind: String::from("open"),
            hidden: false,
            auto_connect: false,
            last_connected_time: String::new(),
        }],
        networks: vec![Network {
            path: network_path.clone(),
            name: String::from("peer-one"),
            connected: false,
            device: station_path.clone(),
            kind: String::from("open"),
            known_network: known_network_path,
            extended_service_set: vec![bss_path],
        }],
        stations: vec![Station {
            path: station_path,
            scanning: false,
            state: String::from("disconnected"),
            connected_network: None,
            ordered_networks: vec![OrderedNetwork {
                path: network_path,
                signal: -4500,
            }],
        }],
    }
}

fn owned_path(path: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(path).expect("test object path should be valid")
}

async fn spawn_test_bus() -> Result<(String, CommandChild)> {
    let config_path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("dbus.conf");
    write(
        &config_path,
        format!(
            r#"
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
    "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
	<allow_anonymous />
	<listen>unix:tmpdir=/tmp</listen>
	<policy context="default">
		<allow receive_type="*" />
		<allow send_type="*" />
		<allow own="*" />
		<allow own_prefix="*" />
	</policy>
	<servicedir>{}</servicedir>
</busconfig>
        "#,
            env!("CARGO_TARGET_TMPDIR"),
        ),
    )
    .await?;

    if !PathBuf::from(DAEMON_PATH).exists() {
        anyhow::bail!("dbus-daemon is not available in the test environment");
    }

    let mut daemon = Command::new(DAEMON_PATH)
        .args([
            &format!("--config-file={}", config_path.display()),
            "--print-address",
        ])
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?;

    let reader = BufReader::new(daemon.stdout.take().expect("dbus-daemon should have stdout"));
    let address = reader
        .lines()
        .next_line()
        .await?
        .ok_or_else(|| anyhow::anyhow!("dbus-daemon did not print an address"))?;

    Ok((address, daemon))
}

type CommandChild = tokio::process::Child;
