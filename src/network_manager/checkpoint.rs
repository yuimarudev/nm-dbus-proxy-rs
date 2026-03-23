use zbus::{interface, zvariant::OwnedObjectPath};

use crate::runtime::Runtime;

#[derive(Clone, Debug, Default)]
pub struct Checkpoint {
    pub created: i64,
    pub devices: Vec<OwnedObjectPath>,
    pub path: OwnedObjectPath,
    pub rollback_timeout: u32,
    pub runtime: Runtime,
}

#[interface(name = "org.freedesktop.NetworkManager.Checkpoint")]
impl Checkpoint {
    #[zbus(property)]
    fn created(&self) -> i64 {
        self.runtime
            .checkpoint(&self.path)
            .map(|record| record.created)
            .unwrap_or(self.created)
    }

    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        self.runtime
            .checkpoint(&self.path)
            .map(|record| record.devices)
            .unwrap_or_else(|| self.devices.clone())
    }

    #[zbus(property)]
    fn rollback_timeout(&self) -> u32 {
        self.runtime
            .checkpoint_rollback_timeout(
                &self.path,
                crate::network_manager::current_boottime_millis(),
            )
            .unwrap_or(self.rollback_timeout)
    }
}
