use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Order, StdResult, Storage};
use cosmwasm_storage::{
    singleton, singleton_read, Bucket, ReadonlyBucket, Singleton,
};

use spectrum_protocol::deployer::{CodeInfo};

static KEY_CONFIG: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub operator: CanonicalAddr,
    pub time_lock: u64,
}

pub fn config_store(storage: &mut dyn Storage) -> Singleton<Config> {
    singleton(storage, KEY_CONFIG)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}

static KEY_CODE: &[u8] = b"code";

pub fn read_codes<'a>(
    storage: &'a dyn Storage,
    contract_addr: CanonicalAddr,
) -> StdResult<Vec<CodeInfo>> {

    let codes: ReadonlyBucket<'a, CodeInfo> = ReadonlyBucket::multilevel(
        storage,
        &[KEY_CODE, contract_addr.as_slice()],
    );
    codes
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (_, v) = item?;
            Ok(v)
        })
        .collect()
}

pub fn code_store(storage: &mut dyn Storage, contract_addr: CanonicalAddr) -> Bucket<CodeInfo> {
    Bucket::multilevel(storage, &[KEY_CODE, contract_addr.as_slice()])
}
