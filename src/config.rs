use crate::crank::{get_keys_for_market, MarketPubkeys};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use simplelog::*;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{read_keypair_file, Keypair};
use std::fs::File;
use std::sync::Arc;
use std::{fs, str::FromStr};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Configuration {
    pub http_rpc_url: String,
    pub ws_rpc_url: String,
    pub key_path: String,
    pub log_file: String,
    pub debug_log: bool,
    pub crank: Crank,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crank {
    pub markets: Vec<Market>,
    pub dex_program: String,
    pub max_queue_length: u64,
    pub max_wait_for_events_delay: u64,
    pub num_accounts: usize,
    pub events_per_worker: usize,
    pub num_workers: usize,
    pub max_markets_per_tx: usize,
}

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct Market {
    pub name: String,
    pub market_account: String,
    pub coin_wallet: String,
    pub pc_wallet: String,
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct ParsedMarketKeys {
    pub keys: MarketPubkeys,
    pub coin_wallet: Pubkey,
    pub pc_wallet: Pubkey,
}

impl Crank {
    pub fn market_keys(
        &self,
        rpc: &Arc<RpcClient>,
        program_id: Pubkey,
    ) -> Result<Vec<ParsedMarketKeys>> {
        let mut markets = vec![];
        for market in self.markets.iter() {
            let market_keys = get_keys_for_market(
                &rpc,
                &program_id,
                &Pubkey::from_str(market.market_account.as_str()).unwrap(),
            )?;
            markets.push(ParsedMarketKeys {
                keys: market_keys,
                coin_wallet: Pubkey::from_str(market.coin_wallet.as_str()).unwrap(),
                pc_wallet: Pubkey::from_str(market.pc_wallet.as_str()).unwrap(),
            })
        }
        Ok(markets)
    }
}

impl Configuration {
    pub fn new(path: &str, as_json: bool) -> Result<()> {
        let config = Configuration::default();
        config.save(path, as_json)
    }
    pub fn save(&self, path: &str, as_json: bool) -> Result<()> {
        let data = if as_json {
            serde_json::to_string_pretty(&self)?
        } else {
            serde_yaml::to_string(&self)?
        };
        fs::write(path, data).expect("failed to write to file");
        Ok(())
    }
    pub fn load(path: &str, from_json: bool) -> Result<Configuration> {
        let data = fs::read(path).expect("failed to read file");
        let config: Configuration = if from_json {
            serde_json::from_slice(data.as_slice())?
        } else {
            serde_yaml::from_slice(data.as_slice())?
        };
        Ok(config)
    }
    pub fn payer(&self) -> Keypair {
        read_keypair_file(self.key_path.clone()).expect("failed to read keypair file")
    }
    /// if file_log is true, log to both file and stdout
    /// otherwise just log to stdout
    pub fn init_log(&self, file_log: bool) -> Result<()> {
        if !file_log {
            if self.debug_log {
                TermLogger::init(
                    LevelFilter::Debug,
                    ConfigBuilder::new()
                        .set_location_level(LevelFilter::Debug)
                        .build(),
                    TerminalMode::Mixed,
                    ColorChoice::Auto,
                )?;
                return Ok(());
            } else {
                TermLogger::init(
                    LevelFilter::Info,
                    ConfigBuilder::new()
                        .set_location_level(LevelFilter::Error)
                        .build(),
                    TerminalMode::Mixed,
                    ColorChoice::Auto,
                )?;
                return Ok(());
            }
        }
        if self.debug_log {
            CombinedLogger::init(vec![
                TermLogger::new(
                    LevelFilter::Debug,
                    ConfigBuilder::new()
                        .set_location_level(LevelFilter::Debug)
                        .build(),
                    TerminalMode::Mixed,
                    ColorChoice::Auto,
                ),
                WriteLogger::new(
                    LevelFilter::Debug,
                    ConfigBuilder::new()
                        .set_location_level(LevelFilter::Debug)
                        .build(),
                    File::create(self.log_file.as_str()).unwrap(),
                ),
            ])?;
        } else {
            CombinedLogger::init(vec![
                TermLogger::new(
                    LevelFilter::Info,
                    ConfigBuilder::new()
                        .set_location_level(LevelFilter::Error)
                        .build(),
                    TerminalMode::Mixed,
                    ColorChoice::Auto,
                ),
                WriteLogger::new(
                    LevelFilter::Info,
                    ConfigBuilder::new()
                        .set_location_level(LevelFilter::Error)
                        .build(),
                    File::create(self.log_file.as_str()).unwrap(),
                ),
            ])?;
        }

        Ok(())
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            http_rpc_url: "https://api.devnet.solana.com".to_string(),
            ws_rpc_url: "ws://api.devnet.solana.com".to_string(),
            key_path: "~/.config/solana/id.json".to_string(),
            log_file: "liquidator.log".to_string(),
            debug_log: false,
            crank: Crank::default(),
        }
    }
}

impl Default for Crank {
    fn default() -> Self {
        Self {
            dex_program: "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin".to_string(),
            markets: vec![Market {
                name: "TULIP-USDC".to_string(),
                market_account: "somekey".to_string(),
                coin_wallet: "somewallet".to_string(),
                pc_wallet: "some_pc_wallet".to_string(),
            }],
            num_workers: 1,
            max_queue_length: 1,
            max_wait_for_events_delay: 60,
            num_accounts: 32,
            events_per_worker: 5,
            max_markets_per_tx: 6,
        }
    }
}
