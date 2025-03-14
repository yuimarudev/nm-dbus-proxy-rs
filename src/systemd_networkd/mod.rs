use anyhow::Result;
use zbus::{Connection, zvariant::OwnedObjectPath};

pub mod link;

use link::{Link, LinkDescription};
use zbus_systemd::network1::{LinkProxy, ManagerProxy};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Manager {
    pub links: Vec<Link>,
}

impl Manager {
    pub async fn request(conn: &Connection) -> Result<Self> {
        let manager = ManagerProxy::new(conn).await?;

        let link_paths: Vec<OwnedObjectPath> = manager
            .list_links()
            .await?
            .iter()
            .map(|(_i, _id, path)| path)
            .cloned()
            .collect();

        let mut links = vec![];
        for path in link_paths {
            let link = LinkProxy::new(conn, path).await?;
            let desc_text = link.describe().await?;
            let description: LinkDescription = serde_json::from_str(&desc_text)?;
            links.push(Link {
                address_state: link.address_state().await?,
                administrative_state: link.administrative_state().await?,
                bit_rates: link.bit_rates().await?,
                carrier_state: link.carrier_state().await?,
                description,
                ipv4_address_state: link.i_pv4_address_state().await?,
                ipv6_address_state: link.i_pv6_address_state().await?,
                online_state: link.online_state().await?,
                operational_state: link.operational_state().await?,
            });
        }

        Ok(Self { links })
    }
}
