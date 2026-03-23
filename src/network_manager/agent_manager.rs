use zbus::{fdo, interface, message::Header};

use crate::runtime::{RegisteredAgent, Runtime};

#[derive(Clone, Debug, Default)]
pub struct AgentManager {
    pub runtime: Runtime,
}

#[interface(name = "org.freedesktop.NetworkManager.AgentManager")]
impl AgentManager {
    fn register(
        &self,
        identifier: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        if identifier.is_empty() {
            return Err(fdo::Error::InvalidArgs(String::from(
                "agent identifier must not be empty",
            )));
        }

        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::Failed(String::from("missing D-Bus sender")))?;

        self.runtime.add_registered_agent(RegisteredAgent {
            capabilities: 0,
            identifier: identifier.to_string(),
            sender: sender.to_string(),
        });

        Ok(())
    }

    fn register_with_capabilities(
        &self,
        identifier: &str,
        capabilities: u32,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::Failed(String::from("missing D-Bus sender")))?;
        let sender = sender.to_string();
        self.register(identifier, header)?;
        self.runtime.remove_registered_agent(sender.as_str());
        self.runtime.add_registered_agent(RegisteredAgent {
            capabilities,
            identifier: identifier.to_string(),
            sender,
        });

        Ok(())
    }

    fn unregister(&self, #[zbus(header)] header: Header<'_>) -> fdo::Result<()> {
        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::Failed(String::from("missing D-Bus sender")))?;
        self.runtime
            .remove_registered_agent(sender.as_str())
            .ok_or_else(|| fdo::Error::Failed(String::from("unknown secret agent")))?;
        Ok(())
    }
}
