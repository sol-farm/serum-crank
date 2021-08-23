use std::{sync::Arc};
use anyhow::{Result, anyhow};
use solana_sdk::{self, pubkey::Pubkey};
use solana_client::{self, rpc_client::RpcClient};


pub struct Crank {
    pub dex_program_id: Pubkey,
    pub market: Pubkey,
    pub coin_wallet: Pubkey,
    pub pc_wallet: Pubkey,
    pub payer: String,
    pub log_directory: String,
    pub max_q_length: u64,
    pub max_wait_for_events_delay: u64,
    pub num_accounts: usize,
    pub events_per_worker: usize,
    pub num_workers: usize,
}

impl Crank {
    pub fn new(
        dex_program_id: Pubkey,
        market: Pubkey,
        coin_wallet: Pubkey,
        pc_wallet: Pubkey,
        payer: String,
        log_directory: String,
        max_q_length: Option<u64>,
        max_wait_for_events_delay: Option<u64>,
        num_accounts: Option<usize>,
        events_per_worker: usize,
        num_workers: usize,
    ) -> Arc<Self> {
        Arc::new(Self{
            dex_program_id,
            market,
            coin_wallet,
            pc_wallet,
            payer,
            log_directory,
            events_per_worker,
            num_workers,
            max_q_length: max_q_length.unwrap_or(1),
            max_wait_for_events_delay: max_wait_for_events_delay.unwrap_or(60),
            num_accounts: num_accounts.unwrap_or(32),
        })
    }
    pub fn start(self: &Arc<Self>, rpc_url: String) -> Result<()> {
        let rpc_client = RpcClient::new(rpc_url);

        Ok(())
    }
}