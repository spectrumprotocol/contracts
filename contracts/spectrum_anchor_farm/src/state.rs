use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Decimal, StdResult, Storage, Uint128};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read, Bucket, ReadonlyBucket, Singleton,
};

static KEY_CONFIG: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub terraswap_factory: CanonicalAddr,
    pub spectrum_token: CanonicalAddr,
    pub spectrum_gov: CanonicalAddr,
    pub anchor_token: CanonicalAddr,
    pub anchor_staking: CanonicalAddr,
    pub anchor_gov: CanonicalAddr,
    pub controller: CanonicalAddr,
    pub platform: CanonicalAddr,
    pub base_denom: String,
    pub community_fee: Decimal,
    pub platform_fee: Decimal,
    pub controller_fee: Decimal,
    pub deposit_fee: Decimal,
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

pub fn store_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    singleton(storage, KEY_CONFIG).save(config)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}

static KEY_STATE: &[u8] = b"state";

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
pub struct State {
    pub contract_addr: CanonicalAddr,
    pub previous_spec_share: Uint128,
    pub spec_share_index: Decimal,
    pub total_farm_share: Uint128,
    pub total_weight: u32,
    pub earning: Uint128,
    #[serde(default)] pub earning_spec: Uint128,
}

impl State {
    pub fn calc_farm_share(&self, farm_amount: Uint128, total_farm_amount: Uint128) -> Uint128 {
        if self.total_farm_share.is_zero() || total_farm_amount.is_zero() {
            farm_amount
        } else {
            farm_amount.multiply_ratio(self.total_farm_share, total_farm_amount)
        }
    }
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
    pub total_auto_bond_share: Uint128,
    pub total_stake_bond_share: Uint128,
    pub total_stake_bond_amount: Uint128,
    pub weight: u32,
    pub auto_compound: bool,
    pub farm_share: Uint128,
    pub state_spec_share_index: Decimal,
    pub farm_share_index: Decimal,
    pub auto_spec_share_index: Decimal,
    pub stake_spec_share_index: Decimal,
    pub reinvest_allowance: Uint128,
}

impl PoolInfo {
    pub fn calc_auto_bond_share(&self, auto_bond_amount: Uint128, lp_balance: Uint128) -> Uint128 {
        let total_auto_bond_amount = lp_balance
            .checked_sub(self.total_stake_bond_amount)
            .unwrap();
        if self.total_auto_bond_share.is_zero() || total_auto_bond_amount.is_zero() {
            auto_bond_amount
        } else {
            auto_bond_amount.multiply_ratio(self.total_auto_bond_share, total_auto_bond_amount)
        }
    }

    pub fn calc_stake_bond_share(&self, stake_bond_amount: Uint128) -> Uint128 {
        if self.total_stake_bond_share.is_zero() || self.total_stake_bond_amount.is_zero() {
            stake_bond_amount
        } else {
            stake_bond_amount
                .multiply_ratio(self.total_stake_bond_share, self.total_stake_bond_amount)
        }
    }

    pub fn calc_user_auto_balance(&self, lp_balance: Uint128, auto_bond_share: Uint128) -> Uint128 {
        if self.total_auto_bond_share.is_zero() {
            Uint128::zero()
        } else {
            lp_balance
                .checked_sub(self.total_stake_bond_amount)
                .unwrap()
                .multiply_ratio(auto_bond_share, self.total_auto_bond_share)
        }
    }

    pub fn calc_user_stake_balance(&self, stake_bond_share: Uint128) -> Uint128 {
        if self.total_stake_bond_share.is_zero() {
            Uint128::zero()
        } else {
            self.total_stake_bond_amount
                .multiply_ratio(stake_bond_share, self.total_stake_bond_share)
        }
    }
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
    pub farm_share_index: Decimal,
    pub auto_spec_share_index: Decimal,
    pub stake_spec_share_index: Decimal,
    pub auto_bond_share: Uint128,
    pub stake_bond_share: Uint128,
    pub farm_share: Uint128,
    pub spec_share: Uint128,
    pub accum_spec_share: Uint128,
}

/// returns a bucket with all rewards owned by this owner (query it by owner)
pub fn rewards_store<'a>(
    storage: &'a mut dyn Storage,
    owner: &CanonicalAddr,
) -> Bucket<'a, RewardInfo> {
    Bucket::multilevel(storage, &[PREFIX_REWARD, owner.as_slice()])
}

/// returns a bucket with all rewards owned by this owner (query it by owner)
/// (read-only version for queries)
pub fn rewards_read<'a>(
    storage: &'a dyn Storage,
    owner: &CanonicalAddr,
) -> ReadonlyBucket<'a, RewardInfo> {
    ReadonlyBucket::multilevel(storage, &[PREFIX_REWARD, owner.as_slice()])
}
