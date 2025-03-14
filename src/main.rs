#![deny(clippy::all, clippy::pedantic, unsafe_code)]
#![allow(clippy::unused_self)]

use std::future::pending;

use anyhow::Result;
use clap::Parser;
use zbus::{Address, conn::Builder};

mod enums;
mod network_manager;
mod systemd_networkd;

use nm_dbus_proxy::{start_service, systemd_networkd::Manager};

#[derive(Clone, Debug, Parser, PartialEq, Eq)]
#[command(about, long_about = None, version)]
struct Args {
    /// D-Bus bus for exposing `NetworkManager` API (uses system bus by default)
    #[arg(long)]
    service_bus: Option<Address>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let networkd_bus = Builder::system()?.build().await?;
    let manager = Manager::request(&networkd_bus).await?;

    let _service_bus = start_service(args.service_bus, manager).await?;

    pending::<()>().await;

    Ok(())
}
