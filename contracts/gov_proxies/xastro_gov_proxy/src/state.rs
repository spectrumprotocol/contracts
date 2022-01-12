use cosmwasm_storage::{singleton_read, singleton, Bucket, bucket, bucket_read, Singleton};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Storage, StdResult, Uint128};

pub fn default_addr() -> CanonicalAddr {
    CanonicalAddr::from(vec![])
}

static KEY_CONFIG: &[u8] = b"config";
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    #[serde(default = "default_addr")] pub xastro_token: CanonicalAddr,
    pub farm_token: CanonicalAddr, // Psi token address
    #[serde(default = "default_addr")] pub farm_gov: CanonicalAddr, // Psi gov address
}

pub fn store_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    singleton(storage, KEY_CONFIG).save(config)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}

static KEY_STATE: &[u8] = b"state";
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub total_share: Uint128,
}

impl State {
    pub fn calc_share(&self, total_balance: Uint128, amount: Uint128) -> Uint128 {
        if self.total_share.is_zero() || total_balance.is_zero() {
            amount
        } else {
            amount.multiply_ratio(self.total_share, total_balance)
        }
    }

    pub fn calc_balance(&self, total_balance: Uint128, share: Uint128) -> Uint128 {
        if self.total_share.is_zero() {
            Uint128::zero()
        } else {
            share.multiply_ratio(total_balance, self.total_share)
        }
    }
}

pub fn state_store(storage: &mut dyn Storage) -> Singleton<State> {
    singleton(storage, KEY_STATE)
}

pub fn read_state(storage: &dyn Storage) -> StdResult<State> {
    singleton_read(storage, KEY_STATE).load()
}

static PREFIX_ACCOUNT: &[u8] = b"account";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
pub struct Account {
    pub share: Uint128,                        // total staked balance
}

pub fn account_store(storage: &mut dyn Storage) -> Bucket<Account> {
    bucket(storage, PREFIX_ACCOUNT)
}

pub fn read_account(storage: &dyn Storage, key: &[u8]) -> StdResult<Option<Account>> {
    bucket_read(storage, PREFIX_ACCOUNT).may_load(key)
}
