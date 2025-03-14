#![deny(clippy::all, clippy::pedantic, unsafe_code)]
#![allow(clippy::unused_self)]

use std::future::pending;

use clap::Parser;
use nm_dbus_proxy::start_service;
use zbus::Address;

mod enums;
mod network_manager;

#[derive(Clone, Debug, Parser, PartialEq, Eq)]
#[command(about, long_about = None, version)]
struct Args {
    /// D-Bus bus for exposing `NetworkManager` API (uses system bus by default)
    #[arg(long)]
    service_bus: Option<Address>,
}

#[tokio::main]
async fn main() -> Result<(), zbus::Error> {
    let args = Args::parse();

    let _service_bus = start_service(args.service_bus).await?;

    pending::<()>().await;

    Ok(())
}
