use crate::config::{Configuration, ParsedMarketKeys};
use anyhow::{anyhow, format_err, Result};
use crossbeam::{select, sync::WaitGroup};
use crossbeam_channel::Receiver;
use crossbeam_queue::ArrayQueue;
use log::{error, info, warn};
use safe_transmute::{
    guard::SingleManyGuard,
    to_bytes::{transmute_one_to_bytes, transmute_to_bytes},
    transmute_many, transmute_many_pedantic, transmute_one_pedantic,
};
use serum_dex::instruction::MarketInstruction;
use serum_dex::state::gen_vault_signer_key;
use serum_dex::state::Event;
use serum_dex::state::EventQueueHeader;
use serum_dex::state::QueueHeader;
use serum_dex::state::{AccountFlag, Market, MarketState, MarketStateV2};
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_client::{self, rpc_client::RpcClient};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::signature::Signature;
use solana_sdk::signature::Signer;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::transaction::Transaction;
use solana_sdk::{self, pubkey::Pubkey};
use std::collections::BTreeSet;
use std::convert::identity;
use std::mem::size_of;
use std::str::FromStr;
use std::sync::Arc;

use std::{borrow::Cow, collections::HashMap, sync::RwLock};
use std::{
    ops::Deref,
};
pub struct Crank {
    pub config: Arc<Configuration>,
}

impl Crank {
    pub fn new(config: Arc<Configuration>) -> Arc<Self> {
        Arc::new(Self { config })
    }
    pub fn start(self: &Arc<Self>, exit_chan: Receiver<bool>) -> Result<()> {
        let rpc_client = Arc::new(RpcClient::new(self.config.http_rpc_url.clone()));
        let payer = Arc::new(self.config.payer());
        let dex_program = Pubkey::from_str(self.config.crank.dex_program.as_str()).unwrap();
        let market_keys = self.config.crank.market_keys(&rpc_client, dex_program)?;
        let slot_height_map: Arc<RwLock<HashMap<String, u64>>> =
            Arc::new(RwLock::new(HashMap::new()));
        loop {
            select! {
                recv(exit_chan) -> _msg => {
                    warn!("caught exit signal");
                    return Ok(());
                },
                default => {}
            }
            let work_loop = |market_key: &ParsedMarketKeys| -> Result<Option<Vec<Instruction>>> {
                let mut queue_accounts = rpc_client.get_multiple_accounts_with_commitment(
                    &[market_key.keys.event_q, market_key.keys.req_q],
                    CommitmentConfig::processed(),
                )?;
                if queue_accounts.value.len() != 2 {
                    return Err(anyhow!("failed to find correct number of queue accounts"));
                }
                let event_q_slot = queue_accounts.context.slot;
                {
                    if let Ok(max_height) = slot_height_map.try_read() {
                        if let Some(height) = max_height.get(&market_key.keys.market.to_string()) {
                            if event_q_slot <= *height {
                                info!(
                                    "Skipping crank. Already cranked for slot. Event queue slot: {}, Max seen slot: {}",
                                    event_q_slot, height
                                );
                                return Ok(None);
                            }
                        }
                    } else {
                        return Ok(None);
                    }
                }
                let event_q_data = match std::mem::take(&mut queue_accounts.value[0]) {
                    Some(event_q) => event_q.data,
                    None => {
                        return Err(anyhow!(
                            "{} event q value and context is none, skipping....",
                            market_key.keys.market
                        ));
                    }
                };
                let req_q_data = match std::mem::take(&mut queue_accounts.value[1]) {
                    Some(req_q_data) => req_q_data.data,
                    None => {
                        return Err(anyhow!(
                            "{} request q value and context is none, skipping....",
                            market_key.keys.market
                        ));
                    }
                };
                let inner: Cow<[u64]> = remove_dex_account_padding(&event_q_data)?;
                let (_header, seg0, seg1) = parse_event_queue(&inner)?;
                let req_inner: Cow<[u64]> = remove_dex_account_padding(&req_q_data)?;
                let (_req_header, req_seg0, req_seg1) = parse_event_queue(&req_inner)?;
                let event_q_len = seg0.len() + seg1.len();
                let req_q_len = req_seg0.len() + req_seg1.len();
                info!(
                    "Size of request queue is {}, market {}, coin {}, pc {}",
                    req_q_len, market_key.keys.market, market_key.coin_wallet, market_key.pc_wallet
                );
                if event_q_len == 0 {
                    return Ok(None);
                }
                info!(
                    "Total event queue length: {}, market {}, coin {}, pc {}",
                    event_q_len,
                    market_key.keys.market,
                    market_key.coin_wallet,
                    market_key.pc_wallet
                );
                let accounts = seg0.iter().chain(seg1.iter()).map(|event| event.owner);
                let mut used_accounts = BTreeSet::new();
                for account in accounts {
                    used_accounts.insert(account);
                    if used_accounts.len() >= self.config.crank.num_accounts {
                        break;
                    }
                }
                let orders_accounts: Vec<_> = used_accounts.into_iter().collect();
                info!(
                    "Number of unique order accounts: {}, market {}, coin {}, pc {}",
                    orders_accounts.len(),
                    market_key.keys.market,
                    market_key.coin_wallet,
                    market_key.pc_wallet
                );
                info!(
                    "First 5 accounts: {:?}",
                    orders_accounts
                        .iter()
                        .take(5)
                        .map(hash_accounts)
                        .collect::<Vec::<_>>()
                );

                let mut account_metas = Vec::with_capacity(orders_accounts.len() + 4);
                for pubkey_words in orders_accounts {
                    let pubkey = Pubkey::new(transmute_to_bytes(&pubkey_words));
                    account_metas.push(AccountMeta::new(pubkey, false));
                }
                for pubkey in [
                    &market_key.keys.market,
                    &market_key.keys.event_q,
                    &market_key.coin_wallet,
                    &market_key.pc_wallet,
                ]
                .iter()
                {
                    account_metas.push(AccountMeta::new(**pubkey, false));
                }
                let instructions = consume_events_ix(
                    &dex_program,
                    &payer,
                    account_metas,
                    self.config.crank.events_per_worker,
                )?;
                Ok(Some(instructions))
            };
            info!("starting crank run");
            {
                let market_keys = market_keys.clone();
                let res = crossbeam::thread::scope(|s| {
                    let q: Arc<ArrayQueue<(Vec<Instruction>, Pubkey)>> =
                        Arc::new(ArrayQueue::new(market_keys.len()));
                    let wg = WaitGroup::new();
                    for market_key in market_keys.iter() {
                        let wg = wg.clone();
                        let q = Arc::clone(&q);
                        let market_key = market_key.clone();
                        s.spawn(move |_| {
                            let ixs = work_loop(&market_key);
                            match ixs {
                                Ok(ixs) => match ixs {
                                    Some(ixs) => {
                                        let res = q.push((ixs, market_key.keys.market));
                                        if res.is_err() {
                                            error!("failed to push instruction set onto stack for market {}: {:#?}", market_key.keys.market, res.err());
                                        }
                                    }
                                    None => {
                                        warn!(
                                            "found no instructions for market {}",
                                            market_key.keys.market
                                        );
                                    }
                                },
                                Err(err) => {
                                    error!(
                                        "failed to run work loop for market {}: {:#?}",
                                        market_key.keys.market, err
                                    );
                                }
                            }
                            drop(wg);
                        });
                    }
                    info!("waiting for spawned threads to finish");
                    wg.wait();
                    info!("collecting instructions");
                    // average number of markets per tx seems to be 2
                    // which translates to 4 instructions
                    let mut instructions = Vec::with_capacity(4);
                    let mut instructions_markets = Vec::with_capacity(2);
                    loop {
                        let ix_set = q.pop();
                        if ix_set.is_none() {
                            break;
                        }
                        let ix_set = ix_set.unwrap();
                        instructions.extend_from_slice(&ix_set.0);
                        instructions_markets.push(ix_set.1);
                    }
                    if instructions.len() > 0 {
                        info!(
                            "found instructions for {} markets: {:#?}",
                            instructions.len() / 2,
                            instructions_markets
                        );
                        let run_loop = |instructions: &Vec<Instruction>| -> Result<Signature> {
                            let (recent_hash, _fee_calc) = rpc_client.get_recent_blockhash()?;
                            let txn = Transaction::new_signed_with_payer(
                                &instructions[..],
                                Some(&payer.pubkey()),
                                &[payer.deref()],
                                recent_hash,
                            );
                            info!("sending crank instructions");
                            let signature = rpc_client.send_transaction_with_config(
                                &txn,
                                RpcSendTransactionConfig {
                                    skip_preflight: true,
                                    ..RpcSendTransactionConfig::default()
                                },
                            )?;
                            Ok(signature)
                        };
                        if instructions_markets.len() > 6 {
                            // 6 is theoretically the maximum cranks we can fit into one transactions
                            // so split it up
                            let instructions_chunks = instructions.chunks(instructions.len() / 2);
                            info!("starting chunked crank instruction processing");
                            for (_idx, chunk) in instructions_chunks.enumerate() {
                                let res = run_loop(&chunk.to_vec());
                                if res.is_err() {
                                    error!(
                                        "failed to send chunked crank instruction {:#?}",
                                        res.err()
                                    );
                                } else {
                                    info!("processed chunked instruction {}", res.unwrap(),);
                                }
                            }
                            info!("finished chunked crank instruction processing")
                        } else {
                            if instructions_markets.len() > 0 {
                                let res = run_loop(&instructions);
                                if res.is_err() {
                                    error!("failed to send crank instructions {:#?}", res.err());
                                } else {
                                    info!(
                                        "crank ran {} processed {} instructions for {} markets: {:#?}",
                                        res.unwrap(),
                                        instructions.len(),
                                        instructions_markets.len(),
                                        instructions_markets,
                                    );
                                }
                            } else {
                                warn!("no markets needed cranking");
                            }
                        }
                        // update slot number for any markets included in this crank
                        let slot_number = rpc_client.get_slot();
                        match slot_number {
                            Ok(slot_number) => match slot_height_map.try_write() {
                                Ok(mut height_writer) => {
                                    for market in instructions_markets.iter() {
                                        height_writer.insert(market.to_string(), slot_number);
                                    }
                                }
                                Err(err) => {
                                    error!("failed to claim lock on slot height map {:#?}", err);
                                }
                            },
                            Err(err) => {
                                error!("failed to retrieve slot number {:#?}", err);
                            }
                        }
                    }
                });
                if res.is_err() {
                    error!("failed to run crossbeam scope {:#?}", res.err());
                }
            }
            info!("finished crank run");
            std::thread::sleep(std::time::Duration::from_secs(
                self.config.crank.max_wait_for_events_delay,
            ));
        }
    }
}

fn consume_events_ix(
    program_id: &Pubkey,
    payer: &Keypair,
    account_metas: Vec<AccountMeta>,
    to_consume: usize,
) -> Result<Vec<Instruction>> {
    let instruction_data: Vec<u8> = MarketInstruction::ConsumeEvents(to_consume as u16).pack();
    let instruction = Instruction {
        program_id: *program_id,
        accounts: account_metas,
        data: instruction_data,
    };
    let random_instruction = solana_sdk::system_instruction::transfer(
        &payer.pubkey(),
        &payer.pubkey(),
        rand::random::<u64>() % 10000 + 1,
    );
    Ok(vec![instruction, random_instruction])
}

#[cfg(target_endian = "little")]
pub fn get_keys_for_market<'a>(
    client: &'a RpcClient,
    program_id: &'a Pubkey,
    market: &'a Pubkey,
) -> Result<MarketPubkeys> {
    let account_data: Vec<u8> = client.get_account_data(&market)?;
    let words: Cow<[u64]> = remove_dex_account_padding(&account_data)?;
    let market_state: MarketState = {
        let account_flags = Market::account_flags(&account_data)?;
        if account_flags.intersects(AccountFlag::Permissioned) {
            let state = transmute_one_pedantic::<MarketStateV2>(transmute_to_bytes(&words))
                .map_err(|e| e.without_src())?;
            state.inner
        } else {
            transmute_one_pedantic::<MarketState>(transmute_to_bytes(&words))
                .map_err(|e| e.without_src())?
        }
    };
    market_state.check_flags()?;
    let vault_signer_key =
        gen_vault_signer_key(market_state.vault_signer_nonce, market, program_id)?;
    assert_eq!(
        transmute_to_bytes(&identity(market_state.own_address)),
        market.as_ref()
    );
    Ok(MarketPubkeys {
        market: *market,
        req_q: Pubkey::new(transmute_one_to_bytes(&identity(market_state.req_q))),
        event_q: Pubkey::new(transmute_one_to_bytes(&identity(market_state.event_q))),
        bids: Pubkey::new(transmute_one_to_bytes(&identity(market_state.bids))),
        asks: Pubkey::new(transmute_one_to_bytes(&identity(market_state.asks))),
        coin_vault: Pubkey::new(transmute_one_to_bytes(&identity(market_state.coin_vault))),
        pc_vault: Pubkey::new(transmute_one_to_bytes(&identity(market_state.pc_vault))),
        vault_signer_key: vault_signer_key,
    })
}

#[cfg(target_endian = "little")]
fn remove_dex_account_padding<'a>(data: &'a [u8]) -> Result<Cow<'a, [u64]>> {
    use serum_dex::state::{ACCOUNT_HEAD_PADDING, ACCOUNT_TAIL_PADDING};
    let head = &data[..ACCOUNT_HEAD_PADDING.len()];
    if data.len() < ACCOUNT_HEAD_PADDING.len() + ACCOUNT_TAIL_PADDING.len() {
        return Err(format_err!(
            "dex account length {} is too small to contain valid padding",
            data.len()
        ));
    }
    if head != ACCOUNT_HEAD_PADDING {
        return Err(format_err!("dex account head padding mismatch"));
    }
    let tail = &data[data.len() - ACCOUNT_TAIL_PADDING.len()..];
    if tail != ACCOUNT_TAIL_PADDING {
        return Err(format_err!("dex account tail padding mismatch"));
    }
    let inner_data_range = ACCOUNT_HEAD_PADDING.len()..(data.len() - ACCOUNT_TAIL_PADDING.len());
    let inner: &'a [u8] = &data[inner_data_range];
    let words: Cow<'a, [u64]> = match transmute_many_pedantic::<u64>(inner) {
        Ok(word_slice) => Cow::Borrowed(word_slice),
        Err(transmute_error) => {
            let word_vec = transmute_error.copy().map_err(|e| e.without_src())?;
            Cow::Owned(word_vec)
        }
    };
    Ok(words)
}

pub fn parse_event_queue(data_words: &[u64]) -> Result<(EventQueueHeader, &[Event], &[Event])> {
    let (header_words, event_words) = data_words.split_at(size_of::<EventQueueHeader>() >> 3);
    let header: EventQueueHeader =
        transmute_one_pedantic(transmute_to_bytes(header_words)).map_err(|e| e.without_src())?;
    let events: &[Event] = transmute_many::<_, SingleManyGuard>(transmute_to_bytes(event_words))
        .map_err(|e| e.without_src())?;
    let (tail_seg, head_seg) = events.split_at(header.head() as usize);
    let head_len = head_seg.len().min(header.count() as usize);
    let tail_len = header.count() as usize - head_len;
    Ok((header, &head_seg[..head_len], &tail_seg[..tail_len]))
}

fn hash_accounts(val: &[u64; 4]) -> u64 {
    val.iter().fold(0, |a, b| b.wrapping_add(a))
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct MarketPubkeys {
    pub market: Pubkey,
    pub req_q: Pubkey,
    pub event_q: Pubkey,
    pub bids: Pubkey,
    pub asks: Pubkey,
    pub coin_vault: Pubkey,
    pub pc_vault: Pubkey,
    pub vault_signer_key: Pubkey,
}
