use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Decimal, StdResult, Storage, Uint128, Api};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read, Bucket, ReadonlyBucket, Singleton,
};
use spectrum_protocol::common::{OrderBy, calc_range_start, calc_range_end, calc_range_start_addr, calc_range_end_addr};

static KEY_CONFIG: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub spectrum_token: CanonicalAddr,
    pub spectrum_gov: CanonicalAddr,
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
    pub previous_spec_share: Uint128,
    pub spec_share_index: Decimal, // per weight
    pub total_weight: u32,
}

pub fn state_store(storage: &mut dyn Storage) -> Singleton<State> {
    singleton(storage, KEY_STATE)
}

pub fn read_state(storage: &dyn Storage) -> StdResult<State> {
    singleton_read(storage, KEY_STATE).load()
}

static PREFIX_POOL_INFO: &[u8] = b"pool_info";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    pub staking_token: CanonicalAddr,
    pub total_bond_amount: Uint128,
    pub weight: u32,
    pub state_spec_share_index: Decimal,
    pub spec_share_index: Decimal, // per bond amount
}

pub fn pool_info_store(storage: &mut dyn Storage) -> Bucket<PoolInfo> {
    bucket(storage, PREFIX_POOL_INFO)
}

pub fn pool_info_read(storage: &dyn Storage) -> ReadonlyBucket<PoolInfo> {
    bucket_read(storage, PREFIX_POOL_INFO)
}

static PREFIX_REWARD: &[u8] = b"reward";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfo {
    pub spec_share_index: Decimal,
    pub bond_amount: Uint128,
    pub spec_share: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoWithAddr {
    pub staker_addr: String,
    pub spec_share_index: Decimal,
    pub bond_amount: Uint128,
    pub spec_share: Uint128,
    pub key: Vec<u8>,
}

pub fn rewards_store<'a>(
    storage: &'a mut dyn Storage,
    owner: &CanonicalAddr,
) -> Bucket<'a, RewardInfo> {
    Bucket::multilevel(storage, &[PREFIX_REWARD, owner.as_slice()])
}

pub fn rewards_read<'a>(
    storage: &'a dyn Storage,
    owner: &CanonicalAddr,
) -> ReadonlyBucket<'a, RewardInfo> {
    ReadonlyBucket::multilevel(storage, &[PREFIX_REWARD, owner.as_slice()])
}

const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 100;
pub fn all_rewards_read<'a>(
    api: &dyn Api,
    storage: &'a dyn Storage,
    start_after: Option<CanonicalAddr>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<Vec<RewardInfoWithAddr>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let (start, end, order_by) = match order_by {
        Some(OrderBy::Asc) => (calc_range_start_addr(start_after), None, OrderBy::Asc),
        _ => (None, calc_range_end_addr(start_after), OrderBy::Desc),
    };

        let rewards: ReadonlyBucket<'a, RewardInfo> = ReadonlyBucket::new(storage, PREFIX_REWARD);

        rewards
            .range(start.as_deref(), end.as_deref(), order_by.into())
            .take(limit)
            .map(|item| {
                let (k, v) = item?;
                let res = RewardInfoWithAddr {
                    staker_addr: api.addr_humanize(&CanonicalAddr::from(&k[0..54]))?.to_string(),
                    spec_share_index: v.spec_share_index,
                    bond_amount: v.bond_amount,
                    spec_share: v.spec_share,
                    key: k
                };
                Ok(res)
            })
            .collect()
}
