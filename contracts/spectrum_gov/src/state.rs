use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Decimal, Order, StdResult, Storage, Uint128};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read, Bucket, ReadonlyBucket, Singleton,
};

use spectrum_protocol::common::{
    calc_range_end, calc_range_end_addr, calc_range_start, calc_range_start_addr, OrderBy,
};
use spectrum_protocol::gov::{PollExecuteMsg, PollStatus, VoterInfo};

static KEY_CONFIG: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub spec_token: CanonicalAddr,
    pub quorum: Decimal,
    pub threshold: Decimal,
    pub voting_period: u64,
    pub effective_delay: u64,
    pub expiration_period: u64,
    pub proposal_deposit: Uint128,
    pub mint_per_block: Uint128,
    pub mint_start: u64,
    pub mint_end: u64,
    pub warchest_address: CanonicalAddr,
    pub warchest_ratio: Decimal,
}

pub fn config_store(storage: &mut dyn Storage) -> Singleton<Config> {
    singleton(storage, KEY_CONFIG)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}

static KEY_STATE: &[u8] = b"state";

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
pub struct State {
    pub contract_addr: CanonicalAddr,
    pub poll_count: u64,
    pub total_share: Uint128,
    pub poll_deposit: Uint128,
    pub last_mint: u64,
    pub total_weight: u32,
}

impl State {
    pub fn calc_share(&self, amount: Uint128, total_balance: Uint128) -> Uint128 {
        if self.total_share.is_zero() || total_balance.is_zero() {
            amount
        } else {
            amount.multiply_ratio(self.total_share, total_balance)
        }
    }
}

pub fn state_store(storage: &mut dyn Storage) -> Singleton<State> {
    singleton(storage, KEY_STATE)
}

pub fn read_state(storage: &dyn Storage) -> StdResult<State> {
    singleton_read(storage, KEY_STATE).load()
}

static PREFIX_POLL: &[u8] = b"poll";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Poll {
    pub id: u64,
    pub creator: CanonicalAddr,
    pub status: PollStatus,
    pub yes_votes: Uint128,
    pub no_votes: Uint128,
    pub end_height: u64,
    pub title: String,
    pub description: String,
    pub link: Option<String>,
    pub execute_msgs: Vec<PollExecuteMsg>,
    pub deposit_amount: Uint128,
    pub total_balance_at_end_poll: Option<Uint128>,
}

pub fn poll_store(storage: &mut dyn Storage) -> Bucket<Poll> {
    bucket(storage, PREFIX_POLL)
}

pub fn read_poll(storage: &dyn Storage, key: &[u8]) -> StdResult<Option<Poll>> {
    bucket_read(storage, PREFIX_POLL).may_load(key)
}

static PREFIX_ACCOUNT: &[u8] = b"account";

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Account {
    pub share: Uint128,                        // total staked balance
    pub locked_balance: Vec<(u64, VoterInfo)>, // maps poll_id to weight voted
}

impl Account {
    pub fn calc_balance(&self, total_balance: Uint128, total_share: Uint128) -> Uint128 {
        if total_share.is_zero() {
            Uint128::zero()
        } else {
            self.share.multiply_ratio(total_balance, total_share)
        }
    }
}

pub fn account_store(storage: &mut dyn Storage) -> Bucket<Account> {
    bucket(storage, PREFIX_ACCOUNT)
}

pub fn read_account(storage: &dyn Storage, key: &[u8]) -> StdResult<Option<Account>> {
    bucket_read(storage, PREFIX_ACCOUNT).may_load(key)
}

static PREFIX_POLL_INDEXER: &[u8] = b"poll_indexer";

pub fn poll_indexer_store<'a>(
    storage: &'a mut dyn Storage,
    status: &PollStatus,
) -> Bucket<'a, bool> {
    Bucket::multilevel(
        storage,
        &[PREFIX_POLL_INDEXER, status.to_string().as_bytes()],
    )
}

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;
pub fn read_polls<'a>(
    storage: &'a dyn Storage,
    filter: Option<PollStatus>,
    start_after: Option<u64>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<Vec<Poll>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let (start, end, order_by) = match order_by {
        Some(OrderBy::Asc) => (calc_range_start(start_after), None, OrderBy::Asc),
        _ => (None, calc_range_end(start_after), OrderBy::Desc),
    };

    if let Some(status) = filter {
        let poll_indexer: ReadonlyBucket<'a, bool> = ReadonlyBucket::multilevel(
            storage,
            &[PREFIX_POLL_INDEXER, status.to_string().as_bytes()],
        );
        poll_indexer
            .range(start.as_deref(), end.as_deref(), order_by.into())
            .take(limit)
            .map(|item| {
                let (k, _) = item?;
                Ok(read_poll(storage, &k)?.unwrap())
            })
            .collect()
    } else {
        let polls: ReadonlyBucket<'a, Poll> = ReadonlyBucket::new(storage, PREFIX_POLL);

        polls
            .range(start.as_deref(), end.as_deref(), order_by.into())
            .take(limit)
            .map(|item| {
                let (_, v) = item?;
                Ok(v)
            })
            .collect()
    }
}

static PREFIX_POLL_VOTER: &[u8] = b"poll_voter";

pub fn poll_voter_store(storage: &mut dyn Storage, poll_id: u64) -> Bucket<VoterInfo> {
    Bucket::multilevel(storage,&[PREFIX_POLL_VOTER, &poll_id.to_be_bytes()])
}

pub fn read_poll_voter(
    storage: &dyn Storage,
    poll_id: u64,
    key: &CanonicalAddr,
) -> StdResult<VoterInfo> {
    ReadonlyBucket::multilevel(storage, &[PREFIX_POLL_VOTER, &poll_id.to_be_bytes()])
        .load(key.as_slice())
}

pub fn read_poll_voters<'a>(
    storage: &'a dyn Storage,
    poll_id: u64,
    start_after: Option<CanonicalAddr>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<Vec<(CanonicalAddr, VoterInfo)>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let (start, end, order_by) = match order_by {
        Some(OrderBy::Asc) => (calc_range_start_addr(start_after), None, OrderBy::Asc),
        _ => (None, calc_range_end_addr(start_after), OrderBy::Desc),
    };

    let voters: ReadonlyBucket<'a, VoterInfo> =
        ReadonlyBucket::multilevel(storage, &[PREFIX_POLL_VOTER, &poll_id.to_be_bytes()]);
    voters
        .range(start.as_deref(), end.as_deref(), order_by.into())
        .take(limit)
        .map(|item| {
            let (k, v) = item?;
            Ok((CanonicalAddr::from(k), v))
        })
        .collect()
}

static PREFIX_VAULT: &[u8] = b"vault";

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Vault {
    pub weight: u32,
}

pub fn vault_store(storage: &mut dyn Storage) -> Bucket<Vault> {
    bucket(storage, PREFIX_VAULT)
}

pub fn read_vault(storage: &dyn Storage, key: &[u8]) -> StdResult<Option<Vault>> {
    bucket_read(storage, PREFIX_VAULT).may_load(key)
}

pub fn read_vaults(storage: &dyn Storage) -> StdResult<Vec<(CanonicalAddr, Vault)>> {
    bucket_read(storage, PREFIX_VAULT)
        .range(None, None, Order::Descending)
        .map(|item| {
            let (k, v) = item?;
            Ok((CanonicalAddr::from(k), v))
        })
        .collect()
}
