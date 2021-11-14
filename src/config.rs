use std::{collections::HashSet, net::SocketAddr};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Deserializer};
use tokio::fs;

fn deserialize_address<'de, D>(de: D) -> Result<SocketAddr, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: String = Deserialize::deserialize(de)?;
    crate::parse_address(&raw)
        .ok_or_else(|| serde::de::Error::custom(anyhow!("Invalid IP address.")))
}

#[derive(Deserialize, Debug)]
pub struct Device {
    alias: String,

    #[serde(deserialize_with = "deserialize_address")]
    address: SocketAddr,
}

#[derive(Deserialize, Default)]
struct ConfigInner {
    default_device: Option<String>,
    devices: Vec<Device>,
}

#[derive(Default, Debug)]
pub struct Config {
    pub default_device: Option<SocketAddr>,
    pub devices: Vec<Device>,
}

impl Config {
    pub async fn load() -> Result<Config> {
        let proj_dirs = directories::ProjectDirs::from("com", "psr31", "lifxc");
        let config_path = proj_dirs
            .as_ref()
            .map(|d| d.config_dir().join("config.toml"));

        let config: ConfigInner = match config_path {
            Some(ref p) => match fs::read_to_string(p).await {
                Ok(config) => Some(toml::from_str(&config)?),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.into()),
            },
            None => None,
        }
        .unwrap_or_default();

        // Check each alias is only used once
        let mut seen = HashSet::new();
        for device in &config.devices {
            if !seen.insert(device.alias.as_str()) {
                return Err(anyhow!(
                    "Device alias '{}' is used multiple times.",
                    device.alias
                ));
            }
        }

        let default_device = config
            .default_device
            .as_deref()
            .map(|dstr| {
                if let Some(device) = config.devices.iter().find(|d| d.alias == dstr) {
                    Ok(device.address)
                } else {
                    crate::parse_address(dstr).ok_or_else(|| {
                        anyhow!("Default device is neither a valid IP address or alias.")
                    })
                }
            })
            .transpose()?;

        Ok(Config {
            default_device,
            devices: config.devices,
        })
    }

    pub fn find_alias(&self, alias: &str) -> Option<SocketAddr> {
        self.devices
            .iter()
            .find(|d| d.alias == alias)
            .map(|d| d.address)
    }
}
