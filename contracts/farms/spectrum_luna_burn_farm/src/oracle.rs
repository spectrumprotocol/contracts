use cosmwasm_std::{Addr, Decimal, QuerierWrapper, QueryRequest, StdResult, to_binary, WasmQuery};
use terraswap::asset::{AssetInfo as TsAssetInfo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetInfoBase<T> {
    Cw20 {
        contract_addr: T,
    },
    Native {
        denom: String,
    },
}

pub type AssetInfoUnchecked = AssetInfoBase<String>;
pub type AssetInfo = AssetInfoBase<Addr>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AssetPriceResponse {
    pub price: Decimal,
    pub display_price: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OracleQueryMsg {
    AssetPrice {
        asset_info: AssetInfoUnchecked,
        execute_mode: bool,
    },
}

fn to_oracle_asset_info(asset_info: &TsAssetInfo) -> AssetInfoUnchecked {
    match asset_info {
        TsAssetInfo::Token { contract_addr } => AssetInfoUnchecked::Cw20 {
            contract_addr: contract_addr.to_string(),
        },
        TsAssetInfo::NativeToken { denom } => AssetInfoUnchecked::Native {
            denom: denom.to_string(),
        },
    }
}

pub fn query_price(
    querier: &QuerierWrapper,
    contract_addr: String,
    asset_info: &TsAssetInfo,
    execute_mode: bool,
) -> StdResult<Decimal> {
    let response: AssetPriceResponse =
        querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr,
            msg: to_binary(&OracleQueryMsg::AssetPrice {
                asset_info: to_oracle_asset_info(asset_info),
                execute_mode,
            })?,
        }))?;

    Ok(response.price)
}
