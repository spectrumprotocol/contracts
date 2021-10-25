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
    pub mirror_token: CanonicalAddr,
    pub mirror_staking: CanonicalAddr,
    pub mirror_gov: CanonicalAddr,
    pub controller: CanonicalAddr,
    pub platform: CanonicalAddr,
    pub base_denom: String,
    pub community_fee: Decimal,
    pub platform_fee: Decimal,
    pub controller_fee: Decimal,
    pub deposit_fee: Decimal,
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
    // addr for contract, this is for query
    pub contract_addr: CanonicalAddr,

    // this is to reconcile with gov
    pub previous_spec_share: Uint128,

    // amount of SPEC reward per weight
    pub spec_share_index: Decimal,

    // current MIR rewards in share
    pub total_farm_share: Uint128,

    // total SPEC reward distribution weight
    pub total_weight: u32,

    // earning in ust
    pub earning: Uint128,

    // earning in spec
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
    // LP token
    pub staking_token: CanonicalAddr,

    // total auto-compound share in the pool
    pub total_auto_bond_share: Uint128,

    // total auto-stake share in the pool
    pub total_stake_bond_share: Uint128,

    // LP amount for auto-stake
    pub total_stake_bond_amount: Uint128,

    // distribution weight
    pub weight: u32,

    // current MIR reward share for the pool
    pub farm_share: Uint128,

    // index to reconcile with state.spec_share_index
    // (state.spec_share_index - pool_info.state_spec_share_index) * pool_info.weight = additional SPEC rewards for this pool
    pub state_spec_share_index: Decimal,

    // total MIR rewards in share per total_stake_bond_share
    pub farm_share_index: Decimal,

    // additional SPEC rewards allocated for auto-compound per total_auto_bond_share
    pub auto_spec_share_index: Decimal,

    // additional SPEC rewards allocated for auto-stake per total_stake_bond_share
    pub stake_spec_share_index: Decimal,

    // for MIR pool: number of MIR to reinvest
    // for non-MIR pool: number of UST to reinvest
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
    // index to reconcile with pool_info.farm_share_index
    // (pool_info.farm_share_index - reward_info.farm_share_index) * reward_info.stake_bond_share
    // = new MIR rewards for auto-stake
    pub farm_share_index: Decimal,

    // index to reconcile with pool_info.auto_spec_share_index
    // (pool_info.auto_spec_share_index - reward_info.auto_spec_share_index) * reward_info.auto_bond_share
    // = new SPEC rewards for auto-compound
    pub auto_spec_share_index: Decimal,

    // index to reconcile with pool_info.stake_spec_share_index
    // (pool_info.stake_spec_share_index - reward_info.stake_spec_share_index) * reward_info.stake_bond_share
    // = new SPEC rewards for auto-stake
    pub stake_spec_share_index: Decimal,

    // share of auto-compound for a person
    pub auto_bond_share: Uint128,

    // share of auto-stake for a person
    pub stake_bond_share: Uint128,

    // current MIR rewards in share balance
    pub farm_share: Uint128,

    // current SPEC reward share balance
    pub spec_share: Uint128,
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
