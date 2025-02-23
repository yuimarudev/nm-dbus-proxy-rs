#![deny(clippy::all, clippy::pedantic, unsafe_code)]

use zbus::{conn::Builder, interface};

struct NetworkManager;

#[interface(name = "org.freedesktop.NetworkManager")]
impl NetworkManager {}

#[tokio::main]
async fn main() -> Result<(), zbus::Error> {
    let nm = NetworkManager;

    let _conn = Builder::system()?
        .name("org.freedesktop.NetworkManager")?
        .serve_at("/org/freedesktop/NetworkManager", nm)?
        .build()
        .await?;

    Ok(())
}
