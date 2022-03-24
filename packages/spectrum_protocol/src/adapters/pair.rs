use cosmwasm_std::{to_binary, Addr, Api, Coin, QuerierWrapper, QueryRequest, StdError, StdResult, Uint128, WasmMsg, WasmQuery, CosmosMsg, Decimal};
use cw20::Cw20ExecuteMsg;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::asset::{Asset as AstroportAsset, AssetInfo as AstroportAssetInfo};
use astroport::pair::{Cw20HookMsg, ExecuteMsg, PoolResponse, QueryMsg, ReverseSimulationResponse, SimulationResponse};

use crate::adapters::{Asset, AssetInfo};

//--------------------------------------------------------------------------------------------------
// Asset: conversions and comparisons between Fields of Mars asset types and Astroport asset types
//--------------------------------------------------------------------------------------------------

impl From<Asset> for AstroportAsset {
    fn from(asset: Asset) -> Self {
        Self {
            info: asset.info.into(),
            amount: asset.amount,
        }
    }
}

impl From<AssetInfo> for AstroportAssetInfo {
    fn from(asset_info: AssetInfo) -> Self {
        match asset_info {
            AssetInfo::Cw20 {
                contract_addr,
            } => Self::Token {
                contract_addr,
            },
            AssetInfo::Native {
                denom,
            } => Self::NativeToken {
                denom,
            },
        }
    }
}

impl PartialEq<AssetInfo> for AstroportAssetInfo {
    fn eq(&self, other: &AssetInfo) -> bool {
        match self {
            Self::Token {
                contract_addr,
            } => {
                let self_contract_addr = contract_addr;
                if let AssetInfo::Cw20 {
                    contract_addr,
                } = other
                {
                    self_contract_addr == contract_addr
                } else {
                    false
                }
            }
            Self::NativeToken {
                denom,
            } => {
                let self_denom = denom;
                if let AssetInfo::Native {
                    denom,
                } = other
                {
                    self_denom == denom
                } else {
                    false
                }
            }
        }
    }
}

//--------------------------------------------------------------------------------------------------
// Pair
//--------------------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PairBase<T> {
    /// Address of the Astroport contract_addr contract
    pub contract_addr: T,
    /// Address of the Astroport LP token
    pub share_token: T,
}

pub type PairUnchecked = PairBase<String>;
pub type Pair = PairBase<Addr>;

impl From<Pair> for PairUnchecked {
    fn from(pair: Pair) -> Self {
        PairUnchecked {
            contract_addr: pair.contract_addr.to_string(),
            share_token: pair.share_token.to_string(),
        }
    }
}

impl PairUnchecked {
    pub fn check(&self, api: &dyn Api) -> StdResult<Pair> {
        Ok(Pair {
            contract_addr: api.addr_validate(&self.contract_addr)?,
            share_token: api.addr_validate(&self.share_token)?,
        })
    }
}

impl Pair {
    // INSTANCE CREATION

    pub fn new(contract_addr: &Addr, share_token: &Addr) -> Self {
        Self {
            contract_addr: contract_addr.clone(),
            share_token: share_token.clone(),
        }
    }

    // MESSAGES

    /// Generate messages for providing specified assets
    pub fn provide_msgs(&self, assets: &[Asset; 2], slippage_tolerance: Option<Decimal>) -> StdResult<Vec<CosmosMsg>> {
        let mut msgs: Vec<CosmosMsg> = vec![];
        let mut funds: Vec<Coin> = vec![];

        for asset in assets.iter() {
            match &asset.info {
                AssetInfo::Cw20 {
                    contract_addr,
                } => msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                        spender: self.contract_addr.to_string(),
                        amount: asset.amount,
                        expires: None,
                    })?,
                    funds: vec![],
                })),
                AssetInfo::Native {
                    denom,
                } => funds.push(Coin {
                    denom: denom.clone(),
                    amount: asset.amount,
                }),
            }
        }

        msgs.push(CosmosMsg::Wasm(
            WasmMsg::Execute {
                contract_addr: self.contract_addr.to_string(),
                msg: to_binary(&ExecuteMsg::ProvideLiquidity {
                    assets: [assets[0].clone().into(), assets[1].clone().into()],
                    slippage_tolerance,
                    auto_stake: None,
                    receiver: None,
                })?,
                funds,
            },
        ));

        Ok(msgs)
    }

    /// Generate msg for removing liquidity by burning specified amount of shares
    pub fn withdraw_msg(&self, amount: Uint128) -> StdResult<CosmosMsg> {
        let msg = to_binary(&Cw20ExecuteMsg::Send {
            contract: self.contract_addr.to_string(),
            amount,
            msg: to_binary(&Cw20HookMsg::WithdrawLiquidity {})?,
        })?;
        Ok(CosmosMsg::Wasm(
            WasmMsg::Execute {
                contract_addr: self.share_token.to_string(),
                msg,
                funds: vec![],
            },
        ))
    }

    /// @notice Generate msg for swapping specified asset
    pub fn swap_msg(&self, asset: &Asset, belief_price: Option<Decimal>, max_spread: Option<Decimal>, to: Option<String>) -> StdResult<CosmosMsg> {
        let wasm_msg = match &asset.info {
            AssetInfo::Cw20 {
                contract_addr,
            } => WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: self.contract_addr.to_string(),
                    amount: asset.amount,
                    msg: to_binary(&Cw20HookMsg::Swap {
                        belief_price,
                        max_spread,
                        to,
                    })?,
                })?,
                funds: vec![],
            },

            AssetInfo::Native {
                denom,
            } => WasmMsg::Execute {
                contract_addr: self.contract_addr.to_string(),
                msg: to_binary(&ExecuteMsg::Swap {
                    offer_asset: asset.clone().into(),
                    belief_price,
                    max_spread,
                    to: None,
                })?,
                funds: vec![Coin {
                    denom: denom.clone(),
                    amount: asset.amount,
                }],
            },
        };

        Ok(CosmosMsg::Wasm(wasm_msg))
    }

    // QUERIES

    /// Query an account's balance of the pool's share token
    pub fn query_share(&self, querier: &QuerierWrapper, account: &Addr) -> StdResult<Uint128> {
        AssetInfo::cw20(&self.share_token).query_balance(querier, account)
    }

    /// Query the Astroport pool, parse response, and return the following 3-tuple:
    /// 1. depth of the primary asset
    /// 2. depth of the secondary asset
    /// 3. total supply of the share token
    pub fn query_pool(
        &self,
        querier: &QuerierWrapper,
        primary_asset_info: &AssetInfo,
        secondary_asset_info: &AssetInfo,
    ) -> StdResult<(Uint128, Uint128, Uint128)> {
        let response: PoolResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.contract_addr.to_string(),
            msg: to_binary(&QueryMsg::Pool {})?,
        }))?;

        let primary_asset_depth = response
            .assets
            .iter()
            .find(|asset| &asset.info == primary_asset_info)
            .ok_or_else(|| StdError::generic_err("Cannot find primary asset in pool response"))?
            .amount;

        let secondary_asset_depth = response
            .assets
            .iter()
            .find(|asset| &asset.info == secondary_asset_info)
            .ok_or_else(|| StdError::generic_err("Cannot find secondary asset in pool response"))?
            .amount;

        Ok((primary_asset_depth, secondary_asset_depth, response.total_share))
    }

    pub fn simulate(
        &self,
        querier: &QuerierWrapper,
        asset: &Asset,
    ) -> StdResult<SimulationResponse> {
        querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.contract_addr.to_string(),
            msg: to_binary(&QueryMsg::Simulation {
                offer_asset: asset.clone().into(),
            })?,
        }))
    }

    pub fn reverse_simulate(
        &self,
        querier: &QuerierWrapper,
        asset: &Asset,
    ) -> StdResult<ReverseSimulationResponse> {
        querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.contract_addr.to_string(),
            msg: to_binary(&QueryMsg::ReverseSimulation {
                ask_asset: asset.clone().into(),
            })?,
        }))
    }
}
