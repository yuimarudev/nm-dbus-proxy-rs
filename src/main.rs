// Modified by yuimarudev on 2026-03-23.
// This file contains changes from the original upstream work.
#![deny(clippy::all, clippy::pedantic, unsafe_code)]
#![allow(clippy::unused_self)]

use std::future::pending;

use anyhow::Result;
use clap::Parser;
use zbus::{Address, conn::Builder};

use nm_dbus_proxy::{
    iwd::State as IwdState, spawn_sync_task, start_service_with_runtime, systemd_networkd::Manager,
};

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

    let system_bus = Builder::system()?.build().await?;
    let manager = Manager::request(&system_bus).await?;
    let wireless = IwdState::request(&system_bus).await.unwrap_or_default();

    let (service_bus, runtime) = start_service_with_runtime(args.service_bus, manager, wireless).await?;
    spawn_sync_task(service_bus.clone(), runtime);

    pending::<()>().await;

    Ok(())
}
