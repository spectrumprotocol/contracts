use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, StdResult, Storage};
use cosmwasm_storage::{
    singleton, singleton_read, Singleton,
};

static KEY_CONFIG: &[u8] = b"config";

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub terraswap_factory: CanonicalAddr,
}

pub fn config_store<S: Storage>(storage: &mut S) -> Singleton<S, Config> {
    singleton(storage, KEY_CONFIG)
}

pub fn read_config<S: Storage>(storage: &S) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}
