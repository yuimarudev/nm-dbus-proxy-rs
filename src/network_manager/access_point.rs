// Modified by yuimarudev on 2026-03-23.
// This file contains changes from the original upstream work.
use zbus::interface;

use crate::{enums::NM80211Mode, runtime::Runtime};
use zbus::zvariant::OwnedObjectPath;

#[derive(Clone, Debug, Default)]
pub struct AccessPoint {
    pub bandwidth: u32,
    pub flags: u32,
    pub frequency: u32,
    pub hw_address: String,
    pub last_seen: i32,
    pub max_bitrate: u32,
    pub mode: NM80211Mode,
    pub path: OwnedObjectPath,
    pub rsn_flags: u32,
    pub runtime: Runtime,
    pub ssid: String,
    pub strength: u8,
    pub wpa_flags: u32,
}

/// see: [Device.Wired]( https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.Wired.html )
#[interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl AccessPoint {
    #[zbus(property)]
    fn bandwidth(&self) -> u32 {
        self.runtime
            .access_point(&self.path)
            .map(|record| record.bandwidth)
            .unwrap_or(self.bandwidth)
    }

    #[zbus(property)]
    fn flags(&self) -> u32 {
        self.runtime
            .access_point(&self.path)
            .map(|record| record.flags)
            .unwrap_or(self.flags)
    }

    #[zbus(property)]
    fn frequency(&self) -> u32 {
        self.runtime
            .access_point(&self.path)
            .map(|record| record.frequency)
            .unwrap_or(self.frequency)
    }

    #[zbus(property)]
    fn hw_address(&self) -> String {
        self.runtime
            .access_point(&self.path)
            .map(|record| record.hw_address)
            .unwrap_or_else(|| self.hw_address.clone())
    }

    #[zbus(property)]
    fn last_seen(&self) -> i32 {
        self.runtime
            .access_point(&self.path)
            .map(|record| record.last_seen)
            .unwrap_or(self.last_seen)
    }

    #[zbus(property)]
    fn max_bitrate(&self) -> u32 {
        self.runtime
            .access_point(&self.path)
            .map(|record| record.max_bitrate)
            .unwrap_or(self.max_bitrate)
    }

    #[zbus(property)]
    fn mode(&self) -> u32 {
        self.runtime
            .access_point(&self.path)
            .map(|record| record.mode as u32)
            .unwrap_or(self.mode as u32)
    }

    #[zbus(property)]
    fn rsn_flags(&self) -> u32 {
        self.runtime
            .access_point(&self.path)
            .map(|record| record.rsn_flags)
            .unwrap_or(self.rsn_flags)
    }

    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        self.runtime
            .access_point(&self.path)
            .map(|record| record.ssid.into_bytes())
            .unwrap_or_else(|| self.ssid.as_bytes().to_vec())
    }

    #[zbus(property)]
    fn strength(&self) -> u8 {
        self.runtime
            .access_point(&self.path)
            .map(|record| record.strength)
            .unwrap_or(self.strength)
    }

    #[zbus(property)]
    fn wpa_flags(&self) -> u32 {
        self.runtime
            .access_point(&self.path)
            .map(|record| record.wpa_flags)
            .unwrap_or(self.wpa_flags)
    }
}
