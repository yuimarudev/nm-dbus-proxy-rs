use std::collections::HashMap;

use zbus::{fdo, interface, zvariant::OwnedValue};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Ppp;

#[interface(name = "org.freedesktop.NetworkManager.PPP")]
impl Ppp {
    fn need_secrets(&self) -> (String, String) {
        (String::new(), String::new())
    }

    fn set_ip4_config(&self, _config: HashMap<String, OwnedValue>) -> fdo::Result<()> {
        Ok(())
    }

    fn set_ip6_config(&self, _config: HashMap<String, OwnedValue>) -> fdo::Result<()> {
        Ok(())
    }

    fn set_state(&self, _state: u32) -> fdo::Result<()> {
        Ok(())
    }

    fn set_ifindex(&self, _ifindex: i32) -> fdo::Result<()> {
        Ok(())
    }
}
