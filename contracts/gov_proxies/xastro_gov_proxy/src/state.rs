use cosmwasm_storage::{singleton_read, singleton, Bucket, bucket, bucket_read};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Storage, StdResult, Uint128};

static KEY_CONFIG: &[u8] = b"config";
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub xastro_token: CanonicalAddr,
    pub farm_token: CanonicalAddr, // Psi token address
    pub farm_gov: CanonicalAddr, // Psi gov address
}

pub fn store_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    singleton(storage, KEY_CONFIG).save(config)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
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
