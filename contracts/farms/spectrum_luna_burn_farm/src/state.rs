use std::fmt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, CanonicalAddr, Decimal, StdResult, Storage, Uint128};
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
    pub burn_period: u64,
    pub ust_pair_contract: CanonicalAddr,
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
    // track SPEC reward
    pub previous_spec_share: Uint128,
    pub spec_share_index: Decimal,

    // track luna
    pub total_bond_amount: Uint128,
    pub total_bond_share: Uint128,

    // track unbond
    pub unbonding_amount: Uint128,  // not yet claimable
    pub claimable_amount: Uint128,  // claimable
    pub unbond_counter: u64,        // to assign unbond id
    pub unbonded_index: Uint128,    // index to track claimable
    pub unbonding_index: Uint128,   // index to track new unbonding position

    // misc
    pub deposit_fee: Uint128,       // deposit_fee to send to gov
    pub perf_fee: Uint128,          // fee waiting to send to gov
    pub deposit_earning: Uint128,   // track deposit fee earning in UST
    pub perf_earning: Uint128,      // track perf fee earning in UST
    pub burn_counter: u64,          // to assign burn id
}

pub fn state_store(storage: &mut dyn Storage) -> Singleton<State> {
    singleton(storage, KEY_STATE)
}

pub fn read_state(storage: &dyn Storage) -> StdResult<State> {
    singleton_read(storage, KEY_STATE).load()
}

impl State {
    pub fn get_burnable_amount(&self, balance: Uint128) -> Uint128 {
        self.get_burnable_amount_internal(balance).unwrap_or_default()
    }

    fn get_burnable_amount_internal(&self, balance: Uint128) -> StdResult<Uint128> {
        Ok(balance.checked_sub(self.claimable_amount)?
            .checked_sub(self.deposit_fee)?
            .checked_sub(self.perf_fee)?)
    }

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
    // track SPEC reward
    pub spec_share_index: Decimal,
    pub spec_share: Uint128,

    // track luna & deposited
    pub bond_share: Uint128,
    pub deposit_amount: Uint128,
    pub deposit_time: u64,

    // track unbond
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

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Copy, JsonSchema)]
pub enum HubType {
    bluna,
    lunax,
    cluna,
    stluna,
}

impl fmt::Display for HubType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Hub {
    pub token: Addr,
    pub hub_address: Addr,
    pub hub_type: HubType,
}

static PREFIX_HUB: &[u8] = b"hub";

pub fn hub_store(
    storage: &mut dyn Storage,
) -> Bucket<Hub> {
    bucket(storage, PREFIX_HUB)
}

pub fn hub_read(
    storage: &dyn Storage,
    token: &[u8],
) -> StdResult<Option<Hub>> {
    bucket_read(storage, PREFIX_HUB).may_load(token)
}

pub fn hubs_read(
    storage: &dyn Storage,
) -> ReadonlyBucket<Hub> {
    bucket_read(storage, PREFIX_HUB)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Burn {
    pub id: u64,
    pub batch_id: u64,
    pub input_amount: Uint128,
    pub start_burn: u64,
    pub end_burn: u64,
    pub hub_type: HubType,
    pub hub_address: Addr,
}

static PREFIX_BURN: &[u8] = b"burn";

pub fn burn_store(
    storage: &mut dyn Storage,
) -> Bucket<Burn> {
    bucket(storage, PREFIX_BURN)
}

pub fn burn_read(
    storage: &dyn Storage,
    burn_id: &[u8],
) -> StdResult<Option<Burn>> {
    bucket_read(storage, PREFIX_BURN).may_load(burn_id)
}

pub fn burns_read(
    storage: &dyn Storage,
) -> ReadonlyBucket<Burn> {
    bucket_read(storage, PREFIX_BURN)
}
