use std::ops;

use cosmwasm_std::{
    to_binary, Addr, Api, BalanceResponse, BankMsg, BankQuery, Coin, CosmosMsg, Decimal,
    QuerierWrapper, QueryRequest, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};
use cw20::{BalanceResponse as Cw20BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, TokenInfoResponse};

use terra_cosmwasm::TerraQuerier;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

static DECIMAL_FRACTION: Uint128 = Uint128::new(1_000_000_000_000_000_000u128);

//--------------------------------------------------------------------------------------------------
// AssetInfo
//--------------------------------------------------------------------------------------------------

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

impl From<AssetInfo> for AssetInfoUnchecked {
    fn from(asset_info: AssetInfo) -> Self {
        match &asset_info {
            AssetInfo::Cw20 {
                contract_addr,
            } => AssetInfoUnchecked::Cw20 {
                contract_addr: contract_addr.to_string(),
            },
            AssetInfo::Native {
                denom,
            } => AssetInfoUnchecked::Native {
                denom: denom.clone(),
            },
        }
    }
}

impl AssetInfoUnchecked {
    pub fn check(&self, api: &dyn Api) -> StdResult<AssetInfo> {
        Ok(match self {
            AssetInfoUnchecked::Cw20 {
                contract_addr,
            } => AssetInfo::Cw20 {
                contract_addr: api.addr_validate(contract_addr)?,
            },
            AssetInfoUnchecked::Native {
                denom,
            } => AssetInfo::Native {
                denom: denom.clone(),
            },
        })
    }
}

//--------------------------------------------------------------------------------------------------
// Asset
//--------------------------------------------------------------------------------------------------

impl AssetInfo {
    // INSTANCE CREATION

    pub fn cw20(contract_addr: &Addr) -> Self {
        Self::Cw20 {
            contract_addr: contract_addr.clone(),
        }
    }

    pub fn native(denom: &dyn ToString) -> Self {
        Self::Native {
            denom: denom.to_string(),
        }
    }

    // UTILITIES

    pub fn is_cw20(&self) -> bool {
        match self {
            Self::Cw20 {
                ..
            } => true,
            Self::Native {
                ..
            } => false,
        }
    }

    pub fn is_native(&self) -> bool {
        !self.is_cw20()
    }

    /// Get the asset's label, which is used in `red_bank::msg::DebtResponse`
    /// For native tokens, it's the denom, e.g. uusd, uluna
    /// For CW20 tokens, it's the contract address
    pub fn get_denom(&self) -> String {
        match self {
            AssetInfo::Cw20 {
                contract_addr,
            } => contract_addr.to_string(),
            AssetInfo::Native {
                denom,
            } => denom.clone(),
        }
    }

    /// Get the asset's reference, used in `oracle::msg::QueryMsg::AssetPriceByReference`
    pub fn get_reference(&self) -> Vec<u8> {
        self.get_denom().as_bytes().to_vec()
    }

    // Find how many tokens of the specified denom is sent with the message
    // Zero if the asset is a CW20
    pub fn find_sent_amount(&self, funds: &[Coin]) -> Uint128 {
        let denom = self.get_denom();
        match funds.iter().find(|fund| fund.denom == denom) {
            Some(fund) => fund.amount,
            None => Uint128::zero(),
        }
    }

    // QUERIES

    /// Query an account's balance of the specified asset
    pub fn query_balance(&self, querier: &QuerierWrapper, account: &Addr) -> StdResult<Uint128> {
        match self {
            AssetInfo::Cw20 {
                contract_addr,
            } => {
                let response: Cw20BalanceResponse =
                    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                        contract_addr: contract_addr.to_string(),
                        msg: to_binary(&Cw20QueryMsg::Balance {
                            address: account.to_string(),
                        })?,
                    }))?;
                Ok(response.balance)
            }
            AssetInfo::Native {
                denom,
            } => {
                let response: BalanceResponse =
                    querier.query(&QueryRequest::Bank(BankQuery::Balance {
                        address: account.to_string(),
                        denom: denom.clone(),
                    }))?;
                Ok(response.amount.amount)
            }
        }
    }

    pub fn query_token_decimals(&self, querier: &QuerierWrapper) -> StdResult<u8> {
        Ok(match self {
            AssetInfo::Native { denom: _ } => 6u8,
            AssetInfo::Cw20 { contract_addr } => {
                let res: TokenInfoResponse =
                    querier.query_wasm_smart(contract_addr, &Cw20QueryMsg::TokenInfo {})?;
    
                res.decimals
            }
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AssetBase<T> {
    pub info: AssetInfoBase<T>,
    pub amount: Uint128,
}

pub type AssetUnchecked = AssetBase<String>;
pub type Asset = AssetBase<Addr>;

impl From<Asset> for AssetUnchecked {
    fn from(asset: Asset) -> Self {
        AssetUnchecked {
            info: asset.info.into(),
            amount: asset.amount,
        }
    }
}

impl AssetUnchecked {
    pub fn check(&self, api: &dyn Api) -> StdResult<Asset> {
        Ok(Asset {
            info: self.info.check(api)?,
            amount: self.amount,
        })
    }
}

impl ops::Mul<Decimal> for Asset {
    type Output = Self;
    fn mul(self, rhs: Decimal) -> Self {
        &self * rhs
    }
}

impl ops::Mul<Decimal> for &Asset {
    type Output = Asset;
    fn mul(self, rhs: Decimal) -> Asset {
        Asset {
            info: self.info.clone(),
            amount: self.amount * rhs,
        }
    }
}

impl Asset {
    // INSTANCE CREATION

    pub fn new<A: Into<Uint128>>(asset_info: &AssetInfo, amount: A) -> Self {
        Asset {
            info: asset_info.clone(),
            amount: amount.into(),
        }
    }

    pub fn cw20<A: Into<Uint128>>(contract_addr: &Addr, amount: A) -> Self {
        Asset {
            info: AssetInfo::cw20(contract_addr),
            amount: amount.into(),
        }
    }

    pub fn native<A: Into<Uint128>>(denom: &dyn ToString, amount: A) -> Self {
        Asset {
            info: AssetInfo::native(denom),
            amount: amount.into(),
        }
    }

    // MESSAGES

    /// Generate the message for transferring asset of a specific amount from one account
    /// to another using the `Cw20ExecuteMsg::Transfer` message type
    ///
    /// NOTE: `amount` must have tax deducted BEFORE passing into this function!
    ///
    /// Usage:
    /// let msg = asset.deduct_tax(deps.querier)?.transfer_msg(to, amount)?;
    pub fn transfer_msg(&self, to: &Addr) -> StdResult<CosmosMsg> {
        match &self.info {
            AssetInfo::Cw20 {
                contract_addr,
            } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: to.to_string(),
                    amount: self.amount,
                })?,
                funds: vec![],
            })),
            AssetInfo::Native {
                denom,
            } => Ok(CosmosMsg::Bank(BankMsg::Send {
                to_address: to.to_string(),
                amount: vec![Coin {
                    denom: denom.clone(),
                    amount: self.amount,
                }],
            })),
        }
    }

    /// Generate the message for transferring asset of a specific amount from one account
    /// to another using the `Cw20HandleMsg::TransferFrom` message type
    ///
    /// NOTE: Must have allowance
    pub fn transfer_from_msg(&self, from: &Addr, to: &Addr) -> StdResult<CosmosMsg> {
        match &self.info {
            AssetInfo::Cw20 {
                contract_addr,
            } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: from.to_string(),
                    recipient: to.to_string(),
                    amount: self.amount,
                })?,
                funds: vec![],
            })),
            AssetInfo::Native {
                ..
            } => Err(StdError::generic_err("TransferFrom does not apply to native tokens")),
        }
    }

    // UTILITIES

    /// Check if tokens of the specified amount was sent along a message
    pub fn assert_sent_amount(&self, funds: &[Coin]) -> StdResult<()> {
        if self.amount == self.info.find_sent_amount(funds) {
            Ok(())
        } else {
            Err(StdError::generic_err("sent fund mismatch"))
        }
    }

    pub fn with_tax_info(&self, querier: &QuerierWrapper) -> StdResult<AssetWithTaxInfo> {
        match &self.info {
            AssetInfo::Cw20 { .. } => Ok(AssetWithTaxInfo {
                asset: self.clone(),
                tax_info: TaxInfo::default(),
            }),
            AssetInfo::Native { denom } => {
                if denom == "luna" {
                    Ok(AssetWithTaxInfo {
                        asset: self.clone(),
                        tax_info: TaxInfo::default(),
                    })
                } else {
                    let terra_querier = TerraQuerier::new(querier);
                    let rate = terra_querier.query_tax_rate()?.rate;
                    let cap = terra_querier.query_tax_cap(denom.clone())?.cap;
                    Ok(AssetWithTaxInfo {
                        asset: self.clone(),
                        tax_info: TaxInfo { rate, cap },
                    })
                }
            },
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, JsonSchema)]
pub struct TaxInfo {
    pub rate: Decimal,
    pub cap: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AssetWithTaxInfo {
    pub asset: Asset,
    pub tax_info: TaxInfo,
}

impl AssetWithTaxInfo {

    /// Compute total cost (tax included) for transferring specified amount of asset
    ///
    /// E.g. If tax incurred for transferring 100 UST is 0.5 UST, then return 100.5 UST.
    /// This is the total amount that will be deducted from the sender's account.
    pub fn add_tax(&self) -> Self {
        let tax = std::cmp::min(self.asset.amount * self.tax_info.rate, self.tax_info.cap);

        AssetWithTaxInfo {
            asset: Asset {
                info: self.asset.info.clone(),
                amount: self.asset.amount + tax,
            },
            tax_info: self.tax_info.clone(),
        }
    }

    /// Update the asset amount to reflect the deliverable amount if the asset is to be transferred.
    ///
    /// @dev Modified from
    /// https://github.com/terraswap/terraswap/blob/master/packages/terraswap/src/asset.rs#L58
    pub fn deduct_tax(&self) -> StdResult<Self> {
        let tax = std::cmp::min(
            self.asset.amount.checked_sub(self.asset.amount.multiply_ratio(
                DECIMAL_FRACTION,
                DECIMAL_FRACTION * self.tax_info.rate + DECIMAL_FRACTION,
            ))?,
            self.tax_info.cap,
        );

        Ok(AssetWithTaxInfo {
            asset: Asset {
                info: self.asset.info.clone(),
                amount: self.asset.amount - tax, // `tax` is guaranteed to be smaller than `amount` so no need to handle underflow
            },
            tax_info: self.tax_info.clone(),
        })
    }
}
