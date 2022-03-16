use cosmwasm_std::{Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    QueueUndelegate {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    WithdrawFundsToWallet {
        batch_id: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    State {},
    BatchUndelegation {
        batch_id: u64,
    },
    GetUserUndelegationRecords {
        user_addr: String,
        start_after: Option<u64>,
        limit: Option<u64>,
    }, // return shares & undelegation list.
    GetUserUndelegationInfo {
        user_addr: String,
        batch_id: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryBatchUndelegationResponse {
    pub batch: Option<BatchUndelegationRecord>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetFundsClaimRecord {
    pub user_withdrawal_amount: Uint128,
    pub protocol_fee: Uint128,
    pub undelegated_tokens: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UndelegationInfo {
    pub batch_id: u64,
    pub token_amount: Uint128, // Shares undelegated
}
