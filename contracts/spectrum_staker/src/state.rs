use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, StdResult, Storage};
use cosmwasm_storage::{
    singleton, singleton_read, Singleton,
};

static KEY_CONFIG: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub terraswap_factory: CanonicalAddr,
}

pub fn config_store(storage: &mut dyn Storage) -> Singleton<Config> {
    singleton(storage, KEY_CONFIG)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}
