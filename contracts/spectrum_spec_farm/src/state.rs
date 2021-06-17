use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Decimal, ReadonlyStorage, StdResult, Storage, Uint128};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read, Bucket, ReadonlyBucket, Singleton,
};

static KEY_CONFIG: &[u8] = b"config";

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub spectrum_token: CanonicalAddr,
    pub spectrum_gov: CanonicalAddr,
    pub lock_start: u64,
    pub lock_end: u64,
}

impl Config {
    pub fn calc_locked_reward(&self, total_amount: Uint128, height: u64) -> Uint128 {
        if self.lock_end <= height {
            Uint128::zero()
        } else if self.lock_start >= height {
            total_amount
        } else {
            total_amount.multiply_ratio(self.lock_end - height, self.lock_end - self.lock_start)
        }
    }
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
    pub previous_spec_share: Uint128,
    pub spec_share_index: Decimal, // per weight
    pub total_weight: u32,
}

pub fn state_store<S: Storage>(storage: &mut S) -> Singleton<S, State> {
    singleton(storage, KEY_STATE)
}

pub fn read_state<S: Storage>(storage: &S) -> StdResult<State> {
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

pub fn pool_info_store<S: Storage>(storage: &mut S) -> Bucket<S, PoolInfo> {
    bucket(PREFIX_POOL_INFO, storage)
}

pub fn pool_info_read<S: Storage>(storage: &S) -> ReadonlyBucket<S, PoolInfo> {
    bucket_read(PREFIX_POOL_INFO, storage)
}

static PREFIX_REWARD: &[u8] = b"reward";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfo {
    pub spec_share_index: Decimal,
    pub bond_amount: Uint128,
    pub spec_share: Uint128,
    pub accum_spec_share: Uint128,
}

pub fn rewards_store<'a, S: Storage>(
    storage: &'a mut S,
    owner: &CanonicalAddr,
) -> Bucket<'a, S, RewardInfo> {
    Bucket::multilevel(&[PREFIX_REWARD, owner.as_slice()], storage)
}

pub fn rewards_read<'a, S: ReadonlyStorage>(
    storage: &'a S,
    owner: &CanonicalAddr,
) -> ReadonlyBucket<'a, S, RewardInfo> {
    ReadonlyBucket::multilevel(&[PREFIX_REWARD, owner.as_slice()], storage)
}
