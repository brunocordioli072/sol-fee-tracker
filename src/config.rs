use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub general: GeneralConfig,
    pub network: NetworkConfig,
    pub token: TokenConfig,
    pub buy: BuyConfig,
    pub sell: SellConfig,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TokenConfig {
    pub mint: String,
    pub take_profit: u64,
    pub buy_amount: f64,
    pub slippage: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GeneralConfig {
    pub private_key: String,
    pub percentiles: Vec<u32>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct NetworkConfig {
    pub rpc_url: String,
    pub ws_url: String,
    pub grpc_url: String,
    pub grpc_token: String,
    pub nextblock: NextblockConfig,
    pub send_txs_urls: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct NextblockConfig {
    pub enabled: bool,
    pub auth_token: String,
}


#[derive(Deserialize, Debug, Clone)]
pub struct BuyConfig {
    pub slippage: u64,
    pub unit_limit: u32,
    pub max_block_difference: u64,
    pub dynamic_fee: DynamicFeeConfig,
    pub delay: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SellConfig {
    pub slippage: u64,
    pub unit_limit: u32,
    pub max_block_difference: u64,
    pub dynamic_fee: DynamicFeeConfig,
    pub delay: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DynamicFeeConfig {
    pub percentile: u32,
    pub fee_multiplier: f64,
}

// Global static config instance
static CONFIG: OnceLock<Config> = OnceLock::new();

impl Config {
    /// Initialize the global config - call this once at startup
    pub fn init() -> Result<(), Box<dyn std::error::Error>> {
        let content = fs::read_to_string("config.toml")?;
        let config: Config = toml::from_str(&content)?;
        
        CONFIG.set(config).map_err(|_| "Config already initialized")?;
        Ok(())
    }
    
    /// Get reference to the global config instance
    pub fn get() -> &'static Config {
        CONFIG.get().expect("Config not initialized. Call Config::init() first.")
    }
    
    /// Load config from file (original function for backward compatibility)
    pub fn load() -> Result<Config, Box<dyn std::error::Error>> {
        let content = fs::read_to_string("config.toml")?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

// Keep the original function for backward compatibility
pub fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    Config::load()
}