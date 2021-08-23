use anyhow::{anyhow, format_err, Result};
use log::{debug, error, info, warn};
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
use solana_sdk::{self, pubkey::Pubkey, signature::read_keypair_file};
use std::borrow::Cow;
use std::cmp::{max, min};
use std::collections::BTreeSet;
use std::convert::identity;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::Mutex;
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
        Arc::new(Self {
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

pub fn consume_events_loop(
    opts: &Arc<Crank>,
    client: Arc<RpcClient>,
    program_id: &Pubkey,
    payer_path: &String,
    market: &Pubkey,
    coin_wallet: &Pubkey,
    pc_wallet: &Pubkey,
    num_workers: usize,
    events_per_worker: usize,
    num_accounts: usize,
    max_q_length: u64,
    max_wait_for_events_delay: u64,
) -> Result<()> {
    info!("Getting market keys ...");
    let market_keys = get_keys_for_market(&client, &program_id, &market)?;
    info!("{:#?}", market_keys);
    let pool = threadpool::ThreadPool::new(num_workers);
    let max_slot_height_mutex = Arc::new(Mutex::new(0_u64));
    let mut last_cranked_at = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(max_wait_for_events_delay))
        .unwrap_or(std::time::Instant::now());

    loop {
        std::thread::sleep(std::time::Duration::from_millis(1000));

        let loop_start = std::time::Instant::now();
        let start_time = std::time::Instant::now();
        let event_q_value_and_context =
            client.get_account_with_commitment(&market_keys.event_q, CommitmentConfig::recent())?;
        let event_q_slot = event_q_value_and_context.context.slot;
        let max_slot_height = max_slot_height_mutex.lock().unwrap();
        if event_q_slot <= *max_slot_height {
            info!(
                "Skipping crank. Already cranked for slot. Event queue slot: {}, Max seen slot: {}",
                event_q_slot, max_slot_height
            );
            continue;
        }
        drop(max_slot_height);
        let event_q_data = event_q_value_and_context
            .value
            .ok_or(format_err!("Failed to retrieve account"))?
            .data;
        let req_q_data = client
            .get_account_with_commitment(&market_keys.req_q, CommitmentConfig::recent())?
            .value
            .ok_or(format_err!("Failed to retrieve account"))?
            .data;
        let inner: Cow<[u64]> = remove_dex_account_padding(&event_q_data)?;
        let (_header, seg0, seg1) = parse_event_queue(&inner)?;
        let req_inner: Cow<[u64]> = remove_dex_account_padding(&req_q_data)?;
        let (_req_header, req_seg0, req_seg1) = parse_event_queue(&req_inner)?;
        let event_q_len = seg0.len() + seg1.len();
        let req_q_len = req_seg0.len() + req_seg1.len();
        info!(
            "Size of request queue is {}, market {}, coin {}, pc {}",
            req_q_len, market, coin_wallet, pc_wallet
        );

        if event_q_len == 0 {
            continue;
        } else if std::time::Duration::from_secs(max_wait_for_events_delay)
            .gt(&last_cranked_at.elapsed())
            && (event_q_len as u64) < max_q_length
        {
            info!(
                "Skipping crank. Last cranked {} seconds ago and queue only has {} events. \
                Event queue slot: {}",
                last_cranked_at.elapsed().as_secs(),
                event_q_len,
                event_q_slot
            );
            continue;
        } else {
            info!(
                "Total event queue length: {}, market {}, coin {}, pc {}",
                event_q_len, market, coin_wallet, pc_wallet
            );
            let accounts = seg0.iter().chain(seg1.iter()).map(|event| event.owner);
            let mut used_accounts = BTreeSet::new();
            for account in accounts {
                used_accounts.insert(account);
                if used_accounts.len() >= num_accounts {
                    break;
                }
            }
            let orders_accounts: Vec<_> = used_accounts.into_iter().collect();
            info!(
                "Number of unique order accounts: {}, market {}, coin {}, pc {}",
                orders_accounts.len(),
                market,
                coin_wallet,
                pc_wallet
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
                &market_keys.market,
                &market_keys.event_q,
                coin_wallet,
                pc_wallet,
            ]
            .iter()
            {
                account_metas.push(AccountMeta::new(**pubkey, false));
            }
            debug!("Number of workers: {}", num_workers);
            let end_time = std::time::Instant::now();
            info!(
                "Fetching {} events from the queue took {}",
                event_q_len,
                end_time.duration_since(start_time).as_millis()
            );
            for thread_num in 0..min(num_workers, 2 * event_q_len / events_per_worker + 1) {
                let payer = read_keypair_file(&payer_path);
                if payer.is_err() {
                    return Err(anyhow!("failed to read keypair {:#?}", payer.err()));
                }
                let payer = payer.unwrap();
                let program_id = program_id.clone();
                let client = Arc::clone(&client);
                let account_metas = account_metas.clone();
                let event_q = *market_keys.event_q;
                let max_slot_height_mutex_clone = Arc::clone(&max_slot_height_mutex);
                pool.execute(move || {
                    consume_events_wrapper(
                        &client,
                        &program_id,
                        &payer,
                        account_metas,
                        thread_num,
                        events_per_worker,
                        event_q,
                        max_slot_height_mutex_clone,
                        event_q_slot,
                    )
                });
            }
            pool.join();
            last_cranked_at = std::time::Instant::now();
            info!(
                "Total loop time took {}",
                last_cranked_at.duration_since(loop_start).as_millis()
            );
        }
    }
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
        market: Box::new(*market),
        req_q: Box::new(Pubkey::new(transmute_one_to_bytes(&identity(
            market_state.req_q,
        )))),
        event_q: Box::new(Pubkey::new(transmute_one_to_bytes(&identity(
            market_state.event_q,
        )))),
        bids: Box::new(Pubkey::new(transmute_one_to_bytes(&identity(
            market_state.bids,
        )))),
        asks: Box::new(Pubkey::new(transmute_one_to_bytes(&identity(
            market_state.asks,
        )))),
        coin_vault: Box::new(Pubkey::new(transmute_one_to_bytes(&identity(
            market_state.coin_vault,
        )))),
        pc_vault: Box::new(Pubkey::new(transmute_one_to_bytes(&identity(
            market_state.pc_vault,
        )))),
        vault_signer_key: Box::new(vault_signer_key),
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

#[derive(Debug)]
pub struct MarketPubkeys {
    pub market: Box<Pubkey>,
    pub req_q: Box<Pubkey>,
    pub event_q: Box<Pubkey>,
    pub bids: Box<Pubkey>,
    pub asks: Box<Pubkey>,
    pub coin_vault: Box<Pubkey>,
    pub pc_vault: Box<Pubkey>,
    pub vault_signer_key: Box<Pubkey>,
}

fn consume_events_wrapper(
    client: &RpcClient,
    program_id: &Pubkey,
    payer: &Keypair,
    account_metas: Vec<AccountMeta>,
    thread_num: usize,
    to_consume: usize,
    event_q: Pubkey,
    max_slot_height_mutex: Arc<Mutex<u64>>,
    slot: u64,
) {
    let start = std::time::Instant::now();
    let result = consume_events_once(
        &client,
        program_id,
        &payer,
        account_metas,
        to_consume,
        thread_num,
        event_q,
    );
    match result {
        Ok(signature) => {
            info!(
                "[thread {}] Successfully consumed events after {:?}: {}.",
                thread_num,
                start.elapsed(),
                signature
            );
            let mut max_slot_height = max_slot_height_mutex.lock().unwrap();
            *max_slot_height = max(slot, *max_slot_height);
        }
        Err(err) => {
            error!("[thread {}] Received error: {:?}", thread_num, err);
        }
    };
}

fn consume_events_once(
    client: &RpcClient,
    program_id: &Pubkey,
    payer: &Keypair,
    account_metas: Vec<AccountMeta>,
    to_consume: usize,
    _thread_number: usize,
    _event_q: Pubkey,
) -> Result<Signature> {
    let _start = std::time::Instant::now();
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
    let (recent_hash, _fee_calc) = client.get_recent_blockhash()?;
    let txn = Transaction::new_signed_with_payer(
        &[instruction, random_instruction],
        Some(&payer.pubkey()),
        &[payer],
        recent_hash,
    );

    info!("Consuming events ...");
    let signature = client.send_transaction_with_config(
        &txn,
        RpcSendTransactionConfig {
            skip_preflight: true,
            ..RpcSendTransactionConfig::default()
        },
    )?;
    Ok(signature)
}
