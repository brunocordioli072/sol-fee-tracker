use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub network: NetworkConfig,
}

#[derive(Deserialize, Debug, Clone)]
pub struct NetworkConfig {
    pub grpc_url: String,
    pub grpc_token: String,
    pub max_blocks: usize,
    #[serde(default)]
    pub exclude_accounts: Vec<String>,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

impl Config {
    pub fn init() -> Result<(), Box<dyn std::error::Error>> {
        let content = fs::read_to_string("config.toml")?;
        let config: Config = toml::from_str(&content)?;
        CONFIG.set(config).map_err(|_| "Config already initialized")?;
        Ok(())
    }

    pub fn get() -> &'static Config {
        CONFIG.get().expect("Config not initialized")
    }
}
