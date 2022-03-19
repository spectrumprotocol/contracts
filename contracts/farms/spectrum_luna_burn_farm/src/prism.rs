use cosmwasm_std::{Addr, Decimal, QuerierWrapper, QueryRequest, StdResult, to_binary, WasmQuery};
use cw_asset::{Asset, AssetInfo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use terraswap::pair::SimulationResponse;
use terraswap::asset::{Asset as TsAsset, AssetInfo as TsAssetInfo};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrismExecuteMsg {
    Swap {
        offer_asset: Asset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrismQueryMsg {
    Simulation { offer_asset: Asset },
}

pub fn to_cw_asset(asset: &TsAsset) -> Asset {
    Asset {
        amount: asset.amount,
        info: match &asset.info {
            TsAssetInfo::Token { contract_addr } => AssetInfo::Cw20(Addr::unchecked(contract_addr)),
            TsAssetInfo::NativeToken { denom } => AssetInfo::Native(denom.to_string()),
        }
    }
}

pub fn prism_simulate(querier: &QuerierWrapper, contract_addr: &str, offer_asset: &TsAsset) -> StdResult<SimulationResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: contract_addr.to_string(),
        msg: to_binary(&PrismQueryMsg::Simulation {
            offer_asset: to_cw_asset(offer_asset),
        })?,
    }))
}
