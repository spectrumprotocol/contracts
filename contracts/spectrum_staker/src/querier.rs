use std::convert::TryFrom;
use classic_bindings::TerraQuery;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Binary, Decimal, from_binary, QuerierWrapper, QueryRequest, StdError, StdResult, to_binary, Uint128, WasmQuery};
use crate::math::{AMP_PRECISION, N_COINS};

/// This struct is used to return a query result with the general contract configuration.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Last timestamp when the cumulative prices in the pool were updated
    pub block_time_last: u64,
    /// The pool's parameters
    pub params: Option<Binary>,
}

/// This struct is used to store the stableswap pool configuration.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StablePoolConfig {
    /// The current pool amplification
    pub amp: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns contract configuration settings in a custom [`super::pair::ConfigResponse`] structure.
    Config {},
}

pub fn query_leverage(
    querier: &QuerierWrapper<TerraQuery>,
    contract_addr: String,
) -> StdResult<u64> {
    let res: ConfigResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr,
        msg: to_binary(&QueryMsg::Config {})?,
    }))?;
    let config: StablePoolConfig = from_binary(&res.params.unwrap())?;
    let leverage = u64::try_from((config.amp * Uint128::from(AMP_PRECISION)).u128())
        .map_err(|_| StdError::generic_err("overflow"))?
        .checked_mul(u64::from(N_COINS))
        .unwrap();
    Ok(leverage)
}
