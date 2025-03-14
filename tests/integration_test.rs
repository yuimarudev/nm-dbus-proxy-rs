use std::{env::var, path::PathBuf, process::Stdio};

use anyhow::Result;
use nm_dbus_proxy::{start_service, systemd_networkd::Manager};
use tokio::{
    fs::write,
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use zbus::{Address, conn::Builder};

const DAEMON_PATH: &str = "/usr/bin/dbus-daemon";

#[tokio::test]
async fn network_manager_service() -> Result<()> {
    let config_path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("dbus.conf");
    write(
        &config_path,
        format!(
            r#"
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
    "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
	<allow_anonymous />
	<listen>unix:tmpdir=/tmp</listen>
	<policy context="default">
		<allow receive_type="*" />
		<allow send_type="*" />
	</policy>
	<policy user="root">
		<allow own="*" />
		<allow own_prefix="*" />
	</policy>
	<policy user="{}">
		<allow own="*" />
		<allow own_prefix="*" />
	</policy>
	<servicedir>{}</servicedir>
</busconfig>
        "#,
            var("USER").unwrap_or(String::from("nobody")),
            env!("CARGO_TARGET_TMPDIR"),
        ),
    )
    .await
    .expect("should write config to temporary file");

    let daemon_path = PathBuf::from(DAEMON_PATH);
    if !daemon_path.exists() {
        // bail early, test environment is incompatible
        return Ok(());
    }

    let mut daemon = Command::new("/usr/bin/dbus-daemon")
        .args([
            &format!("--config-file={}", config_path.display()),
            "--print-address",
        ])
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .expect("should start `dbus-daemon`");

    let reader = BufReader::new(
        daemon
            .stdout
            .take()
            .expect("`dbus-daemon` should have stdout"),
    );
    let address = reader
        .lines()
        .next_line()
        .await
        .expect("`dbus-daemon` should have valid UTF-8 stdout")
        .expect("`dbus-daemon` output to stdout");

    let networkd_bus = match Builder::system()?.build().await {
        Ok(ok) => ok,
        Err(err) => {
            // bail early, test environment is incompatible
            eprintln!("{err:?}");
            return Ok(());
        }
    };
    let manager = match Manager::request(&networkd_bus).await {
        Ok(ok) => ok,
        Err(err) => {
            // bail early, test environment is incompatible
            eprintln!("{err:?}");
            return Ok(());
        }
    };

    // TODO: run our service concurrently with the rest of the test
    let _service = start_service(
        Some(Address::try_from(address.as_str()).expect("should parse address from string")),
        manager,
    )
    .await
    .expect("service should attach to D-Bus bus");

    // TODO: construct a client Proxy
    // TODO: use the Proxy to interrogate the service
    // TODO: assert that what we get is what we want

    Ok(())
}
