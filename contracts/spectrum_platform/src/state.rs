use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Decimal, Order, ReadonlyStorage, StdResult, Storage};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read, Bucket, ReadonlyBucket, Singleton,
};

use spectrum_protocol::common::{
    calc_range_end, calc_range_end_addr, calc_range_start, calc_range_start_addr, OrderBy,
};
use spectrum_protocol::platform::{ExecuteMsg, PollStatus, VoterInfo};

static KEY_CONFIG: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub quorum: Decimal,
    pub threshold: Decimal,
    pub voting_period: u64,
    pub effective_delay: u64,
    pub expiration_period: u64,
}

pub fn config_store<S: Storage>(storage: &mut S) -> Singleton<S, Config> {
    singleton(storage, KEY_CONFIG)
}

pub fn read_config<S: Storage>(storage: &S) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}

static KEY_STATE: &[u8] = b"state";

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
pub struct State {
    pub contract_addr: CanonicalAddr,
    pub poll_count: u64,
    pub total_weight: u32,
}

pub fn state_store<S: Storage>(storage: &mut S) -> Singleton<S, State> {
    singleton(storage, KEY_STATE)
}

pub fn read_state<S: Storage>(storage: &S) -> StdResult<State> {
    singleton_read(storage, KEY_STATE).load()
}

static PREFIX_POLL: &[u8] = b"poll";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Poll {
    pub id: u64,
    pub creator: CanonicalAddr,
    pub status: PollStatus,
    pub yes_votes: u32,
    pub no_votes: u32,
    pub end_height: u64,
    pub title: String,
    pub description: String,
    pub link: Option<String>,
    pub execute_msgs: Vec<ExecuteMsg>,
    pub total_balance_at_end_poll: Option<u32>,
}

pub fn poll_store<S: Storage>(storage: &mut S) -> Bucket<S, Poll> {
    bucket(PREFIX_POLL, storage)
}

pub fn read_poll<S: ReadonlyStorage>(storage: &S, key: &[u8]) -> StdResult<Option<Poll>> {
    bucket_read(PREFIX_POLL, storage).may_load(key)
}

static PREFIX_POLL_INDEXER: &[u8] = b"poll_indexer";

pub fn poll_indexer_store<'a, S: Storage>(
    storage: &'a mut S,
    status: &PollStatus,
) -> Bucket<'a, S, bool> {
    Bucket::multilevel(
        &[PREFIX_POLL_INDEXER, status.to_string().as_bytes()],
        storage,
    )
}

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;
pub fn read_polls<'a, S: ReadonlyStorage>(
    storage: &'a S,
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
        let poll_indexer: ReadonlyBucket<'a, S, bool> = ReadonlyBucket::multilevel(
            &[PREFIX_POLL_INDEXER, status.to_string().as_bytes()],
            storage,
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
        let polls: ReadonlyBucket<'a, S, Poll> = ReadonlyBucket::new(PREFIX_POLL, storage);

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

pub fn poll_voter_store<S: Storage>(storage: &mut S, poll_id: u64) -> Bucket<S, VoterInfo> {
    Bucket::multilevel(&[PREFIX_POLL_VOTER, &poll_id.to_be_bytes()], storage)
}

pub fn read_poll_voter<S: ReadonlyStorage>(
    storage: &S,
    poll_id: u64,
    key: &CanonicalAddr,
) -> StdResult<VoterInfo> {
    ReadonlyBucket::multilevel(&[PREFIX_POLL_VOTER, &poll_id.to_be_bytes()], storage)
        .load(key.as_slice())
}

pub fn read_poll_voters<'a, S: ReadonlyStorage>(
    storage: &'a S,
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

    let voters: ReadonlyBucket<'a, S, VoterInfo> =
        ReadonlyBucket::multilevel(&[PREFIX_POLL_VOTER, &poll_id.to_be_bytes()], storage);
    voters
        .range(start.as_deref(), end.as_deref(), order_by.into())
        .take(limit)
        .map(|item| {
            let (k, v) = item?;
            Ok((CanonicalAddr::from(k), v))
        })
        .collect()
}

static PREFIX_BOARD: &[u8] = b"board";

pub fn board_store<S: Storage>(storage: &mut S) -> Bucket<S, u32> {
    bucket(PREFIX_BOARD, storage)
}

pub fn read_board<S: ReadonlyStorage>(storage: &S, key: &[u8]) -> u32 {
    let bucket: ReadonlyBucket<S, u32> = bucket_read(PREFIX_BOARD, storage);
    bucket.may_load(key).unwrap().unwrap_or(0u32)
}

pub fn read_boards<S: ReadonlyStorage>(storage: &S) -> StdResult<Vec<(CanonicalAddr, u32)>> {
    let bucket: ReadonlyBucket<S, u32> = bucket_read(PREFIX_BOARD, storage);
    bucket
        .range(None, None, Order::Descending)
        .map(|item| {
            let (key, value) = item?;
            Ok((CanonicalAddr::from(key), value))
        })
        .collect()
}
