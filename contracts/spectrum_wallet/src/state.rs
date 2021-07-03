use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Decimal, Order, ReadonlyStorage, StdResult, Storage, Uint128};
use cosmwasm_storage::{bucket, bucket_read, singleton, singleton_read, Bucket, Singleton};

static KEY_CONFIG: &[u8] = b"config";

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub spectrum_token: CanonicalAddr,
    pub spectrum_gov: CanonicalAddr,
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
    pub previous_share: Uint128,
    pub share_index: Decimal, // per weight
    pub total_weight: u32,
}

pub fn state_store<S: Storage>(storage: &mut S) -> Singleton<S, State> {
    singleton(storage, KEY_STATE)
}

pub fn read_state<S: Storage>(storage: &S) -> StdResult<State> {
    singleton_read(storage, KEY_STATE).load()
}

static PREFIX_REWARD: &[u8] = b"reward";

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfo {
    pub share_index: Decimal,
    pub share: Uint128,
    pub weight: u32,
}

pub fn reward_store<S: Storage>(storage: &mut S) -> Bucket<S, RewardInfo> {
    bucket(PREFIX_REWARD, storage)
}

pub fn read_reward<S: ReadonlyStorage>(
    storage: &S,
    address: &CanonicalAddr,
) -> StdResult<RewardInfo> {
    bucket_read(PREFIX_REWARD, storage).load(address.as_slice())
}

pub fn read_rewards<S: ReadonlyStorage>(
    storage: &S,
) -> StdResult<Vec<(CanonicalAddr, RewardInfo)>> {
    bucket_read(PREFIX_REWARD, storage)
        .range(None, None, Order::Descending)
        .map(|item| {
            let (k, v) = item?;
            Ok((CanonicalAddr::from(k), v))
        })
        .collect()
}
