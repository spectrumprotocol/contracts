use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Decimal, StdResult, Storage, Uint128};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read, Bucket, ReadonlyBucket, Singleton,
};

pub fn default_addr() -> CanonicalAddr {
    CanonicalAddr::from(vec![])
}

static KEY_CONFIG: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub spectrum_token: CanonicalAddr,
    pub spectrum_gov: CanonicalAddr,
    pub astro_token: CanonicalAddr,
    pub weldo_token: CanonicalAddr,
    pub stluna_token: CanonicalAddr,
    pub astroport_generator: CanonicalAddr,
    pub astroport_router: CanonicalAddr,
    pub xastro_proxy: CanonicalAddr,
    pub gov_proxy: Option<CanonicalAddr>,
    pub controller: CanonicalAddr,
    pub platform: CanonicalAddr,
    pub community_fee: Decimal,
    pub platform_fee: Decimal,
    pub controller_fee: Decimal,
    pub deposit_fee: Decimal,
    pub anchor_market: CanonicalAddr,
    pub aust_token: CanonicalAddr,
    pub pair_contract: CanonicalAddr,
    pub astro_ust_pair_contract: CanonicalAddr,
    pub stluna_weldo_pair_contract: CanonicalAddr,
    pub stluna_uluna_pair_contract: CanonicalAddr,
    pub uluna_uusd_pair_contract: CanonicalAddr
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
    pub total_farm_share: Uint128,  // XASTRO
    pub total_farm2_share: Uint128, // ANC
    pub total_weight: u32,
    pub earning: Uint128,
}

impl State {
    pub fn calc_farm_share(&self, farm_amount: Uint128, total_farm_amount: Uint128) -> Uint128 {
        if self.total_farm_share.is_zero() || total_farm_amount.is_zero() {
            farm_amount
        } else {
            farm_amount.multiply_ratio(self.total_farm_share, total_farm_amount)
        }
    }

    pub fn calc_farm2_share(&self, farm2_amount: Uint128, total_farm2_amount: Uint128) -> Uint128 {
        if self.total_farm2_share.is_zero() || total_farm2_amount.is_zero() {
            farm2_amount
        } else {
            farm2_amount.multiply_ratio(self.total_farm2_share, total_farm2_amount)
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
    pub farm_share: Uint128,
    pub farm2_share: Uint128,
    pub state_spec_share_index: Decimal,
    pub farm_share_index: Decimal,
    pub farm2_share_index: Decimal,
    pub auto_spec_share_index: Decimal,
    pub stake_spec_share_index: Decimal,
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
    pub farm2_share_index: Decimal,
    pub auto_spec_share_index: Decimal,
    pub stake_spec_share_index: Decimal,
    pub auto_bond_share: Uint128,
    pub stake_bond_share: Uint128,
    pub farm_share: Uint128,
    pub farm2_share: Uint128,
    pub spec_share: Uint128,
    #[serde(default)] pub deposit_amount: Uint128,
    #[serde(default)] pub deposit_time: u64,
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

impl RewardInfo {
    pub fn create(pool_info: &PoolInfo) -> RewardInfo {
        RewardInfo {
            farm_share_index: pool_info.farm_share_index,
            farm2_share_index: pool_info.farm2_share_index,
            auto_spec_share_index: pool_info.auto_spec_share_index,
            stake_spec_share_index: pool_info.stake_spec_share_index,
            auto_bond_share: Uint128::zero(),
            stake_bond_share: Uint128::zero(),
            spec_share: Uint128::zero(),
            farm_share: Uint128::zero(),
            farm2_share: Uint128::zero(),
            deposit_amount: Uint128::zero(),
            deposit_time: 0u64,
        }
    }
}
