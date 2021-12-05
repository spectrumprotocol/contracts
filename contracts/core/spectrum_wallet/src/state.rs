use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Decimal, Order, StdResult, Storage, Uint128};
use cosmwasm_storage::{bucket, bucket_read, singleton, singleton_read, Bucket, Singleton};
use spectrum_protocol::wallet::{SharePoolInfo, StatePoolInfo};

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
    pub previous_share: Uint128,
    pub share_index: Decimal, // per weight
    pub total_weight: u32,
    #[serde(default)] pub pools: Vec<StatePoolInfo>,
}

pub fn state_store(storage: &mut dyn Storage) -> Singleton<State> {
    singleton(storage, KEY_STATE)
}

pub fn read_state(storage: &dyn Storage) -> StdResult<State> {
    singleton_read(storage, KEY_STATE).load()
}

static PREFIX_REWARD: &[u8] = b"reward";

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfo {
    #[serde(default)]
    pub amount: Uint128,
    #[serde(default)]
    pub lock_start: u64,
    #[serde(default)]
    pub lock_end: u64,
    #[serde(default)]
    pub lock_amount: Uint128,
    pub share_index: Decimal,
    pub share: Uint128,
    pub weight: u32,
    #[serde(default)] pub pools: Vec<SharePoolInfo>,
}

impl RewardInfo {
    pub fn calc_locked_amount(&self, height: u64) -> Uint128 {
        if self.lock_end <= height {
            Uint128::zero()
        } else if self.lock_start >= height {
            self.lock_amount
        } else {
            self.lock_amount
                .multiply_ratio(self.lock_end - height, self.lock_end - self.lock_start)
        }
    }
}

pub fn reward_store(storage: &mut dyn Storage) -> Bucket<RewardInfo> {
    bucket(storage, PREFIX_REWARD)
}

pub fn read_reward(storage: &dyn Storage, address: &CanonicalAddr) -> StdResult<RewardInfo> {
    bucket_read(storage, PREFIX_REWARD).load(address.as_slice())
}

pub fn read_rewards(storage: &dyn Storage) -> StdResult<Vec<(CanonicalAddr, RewardInfo)>> {
    bucket_read(storage, PREFIX_REWARD)
        .range(None, None, Order::Descending)
        .map(|item| {
            let (k, v) = item?;
            Ok((CanonicalAddr::from(k), v))
        })
        .collect()
}
