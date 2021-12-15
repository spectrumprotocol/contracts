use cosmwasm_storage::{singleton_read, singleton, Singleton};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Storage, StdResult, Uint128};

static KEY_CONFIG: &[u8] = b"config";
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub farm_contract: Option<CanonicalAddr>, // Spectrum Nexus farm address whitelist, only whitelist can execute Stake, Unstake
    pub farm_token: CanonicalAddr, // Psi token address
    pub farm_gov: CanonicalAddr, // Psi gov address
    pub spectrum_gov: CanonicalAddr
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
    pub total_deposit: Uint128,
    pub total_withdraw: Uint128,
}

pub fn state_store(storage: &mut dyn Storage) -> Singleton<State> {
    singleton(storage, KEY_STATE)
}

pub fn read_state(storage: &dyn Storage) -> StdResult<State> {
    singleton_read(storage, KEY_STATE).load()
}