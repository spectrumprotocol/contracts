use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Decimal, StdResult, Storage, Uint128};
use cosmwasm_storage::{bucket, bucket_read, singleton, singleton_read, Bucket, Singleton, ReadonlyBucket};

static KEY_CONFIG: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub spectrum_token: CanonicalAddr,
    pub spectrum_gov: CanonicalAddr,
    pub controller: CanonicalAddr,
    pub platform: CanonicalAddr,
    pub community_fee: Decimal,
    pub platform_fee: Decimal,
    pub controller_fee: Decimal,
    pub deposit_fee: Decimal,
    pub anchor_market: CanonicalAddr,
    pub aust_token: CanonicalAddr,
    pub max_unbond_count: usize,
    pub bluna_token: CanonicalAddr,
    pub stluna_token: CanonicalAddr,
    pub lunax_token: CanonicalAddr,
}

pub fn store_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    singleton(storage, KEY_CONFIG).save(config)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}

static KEY_STATE: &[u8] = b"state";

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
pub struct State {
    pub previous_spec_share: Uint128,
    pub spec_share_index: Decimal,
    pub total_bond_amount: Uint128,
    pub total_bond_share: Uint128,
    pub unbonding_amount: Uint128,
    pub unbond_counter: u64,
    pub unbonded_index: Uint128,
    pub unbonding_index: Uint128,
    pub claimable_amount: Uint128,
    pub earning: Uint128,
}

pub fn state_store(storage: &mut dyn Storage) -> Singleton<State> {
    singleton(storage, KEY_STATE)
}

pub fn read_state(storage: &dyn Storage) -> StdResult<State> {
    singleton_read(storage, KEY_STATE).load()
}

impl State {
    pub fn calc_bond_share(&self, bond_amount: Uint128) -> Uint128 {
        if self.total_bond_share.is_zero() || self.total_bond_amount.is_zero() {
            bond_amount
        } else {
            bond_amount.multiply_ratio(self.total_bond_share, self.total_bond_amount)
        }
    }

    pub fn calc_bond_amount(&self, bond_share: Uint128) -> Uint128 {
        if self.total_bond_share.is_zero() {
            Uint128::zero()
        } else {
            self.total_bond_amount.multiply_ratio(bond_share, self.total_bond_share)
        }
    }
}

static PREFIX_REWARD: &[u8] = b"reward";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfo {
    pub spec_share_index: Decimal,
    pub bond_share: Uint128,
    pub spec_share: Uint128,
    pub deposit_amount: Uint128,
    pub deposit_time: u64,
    pub unbonding_amount: Uint128,
}

/// returns a bucket with all rewards owned by this owner (query it by owner)
pub fn rewards_store(
    storage: &mut dyn Storage,
) -> Bucket<RewardInfo> {
    bucket(storage, PREFIX_REWARD)
}

/// returns a bucket with all rewards owned by this owner (query it by owner)
/// (read-only version for queries)
pub fn rewards_read(
    storage: &dyn Storage,
    owner: &[u8],
) -> StdResult<Option<RewardInfo>> {
    bucket_read(storage, PREFIX_REWARD).may_load(owner)
}

impl RewardInfo {
    pub fn create(state: &State) -> RewardInfo {
        RewardInfo {
            spec_share_index: state.spec_share_index,
            bond_share: Uint128::zero(),
            spec_share: Uint128::zero(),
            deposit_amount: Uint128::zero(),
            deposit_time: 0u64,
            unbonding_amount: Uint128::zero(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
pub struct Unbonding {
    pub id: u64,
    pub time: u64,
    pub amount: Uint128,
    pub unbonding_index: Uint128,
}

impl Unbonding {
    pub fn create(state: &mut State, time: u64, amount: Uint128) -> Unbonding {
        state.unbond_counter += 1u64;
        state.unbonding_index += amount;
        Unbonding {
            id: state.unbond_counter,
            unbonding_index: state.unbonding_index,
            time,
            amount,
        }
    }
}

static PREFIX_USER_UNBONDING: &[u8] = b"user_unbonding";

pub fn user_unbonding_store<'a>(
    storage: &'a mut dyn Storage,
    owner: &CanonicalAddr,
) -> Bucket<'a, Unbonding> {
    Bucket::multilevel(storage, &[PREFIX_USER_UNBONDING, owner.as_slice()])
}

pub fn user_unbonding_read<'a>(
    storage: &'a dyn Storage,
    owner: &CanonicalAddr,
) -> ReadonlyBucket<'a, Unbonding> {
    ReadonlyBucket::multilevel(storage, &[PREFIX_USER_UNBONDING, owner.as_slice()])
}
