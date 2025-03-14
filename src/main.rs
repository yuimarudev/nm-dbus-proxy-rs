#![deny(clippy::all, clippy::pedantic, unsafe_code)]
#![allow(clippy::unused_self)]

use std::future::pending;

use anyhow::Result;
use clap::Parser;
use zbus::Address;

mod enums;
mod network_manager;
mod systemd_networkd;

use nm_dbus_proxy::start_service;

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

    let _service_bus = start_service(args.service_bus).await?;

    pending::<()>().await;

    Ok(())
}
