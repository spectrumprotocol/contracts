use cosmwasm_std::{Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::common::OrderBy;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: String,
    pub spec_token: Option<String>,
    pub quorum: Decimal,
    pub threshold: Decimal,
    pub voting_period: u64,
    pub effective_delay: u64,
    pub expiration_period: u64,
    pub proposal_deposit: Uint128,
    pub mint_per_block: Uint128,
    pub mint_start: u64,
    pub mint_end: u64,
    pub warchest_address: Option<String>,
    pub warchest_ratio: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[allow(clippy::large_enum_variant)]
pub enum ExecuteMsg {
    mint {},
    poll_end {
        poll_id: u64,
    },
    poll_execute {
        poll_id: u64,
    },
    poll_expire {
        poll_id: u64,
    },
    poll_vote {
        poll_id: u64,
        vote: VoteOption,
        amount: Uint128,
    },
    receive(Cw20ReceiveMsg),
    update_config {
        owner: Option<String>,
        spec_token: Option<String>,
        quorum: Option<Decimal>,
        threshold: Option<Decimal>,
        voting_period: Option<u64>,
        effective_delay: Option<u64>,
        expiration_period: Option<u64>,
        proposal_deposit: Option<Uint128>,
        mint_per_block: Option<Uint128>,
        mint_start: Option<u64>,
        mint_end: Option<u64>,
        warchest_address: Option<String>,
        warchest_ratio: Option<Decimal>,
    },
    upsert_vault {
        vault_address: String,
        weight: u32,
    },
    withdraw {
        amount: Option<Uint128>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum VoteOption {
    yes,
    no,
}

impl fmt::Display for VoteOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if *self == VoteOption::yes {
            write!(f, "yes")
        } else {
            write!(f, "no")
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Cw20HookMsg {
    poll_start {
        title: String,
        description: String,
        link: Option<String>,
        execute_msgs: Vec<PollExecuteMsg>,
    },
    stake_tokens {
        staker_addr: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum PollExecuteMsg {
    execute { contract: String, msg: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum PollStatus {
    in_progress,
    passed,
    rejected,
    executed,
    expired,
}

impl fmt::Display for PollStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    balance {
        address: String,
        height: Option<u64>,
    },
    config {},
    poll {
        poll_id: u64,
    },
    polls {
        filter: Option<PollStatus>,
        start_after: Option<u64>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    },
    state {
        height: u64,
    },
    vaults {},
    voters {
        poll_id: u64,
        start_after: Option<String>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VoterInfo {
    pub vote: VoteOption,
    pub balance: Uint128,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct BalanceResponse {
    pub balance: Uint128,
    pub share: Uint128,
    pub locked_balance: Vec<(u64, VoterInfo)>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct PollInfo {
    pub id: u64,
    pub creator: String,
    pub status: PollStatus,
    pub end_height: u64,
    pub title: String,
    pub description: String,
    pub link: Option<String>,
    pub deposit_amount: Uint128,
    pub execute_msgs: Vec<PollExecuteMsg>,
    pub yes_votes: Uint128, // balance
    pub no_votes: Uint128,  // balance
    pub total_balance_at_end_poll: Option<Uint128>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct PollsResponse {
    pub polls: Vec<PollInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct StateInfo {
    pub poll_count: u64,
    pub total_share: Uint128,
    pub poll_deposit: Uint128,
    pub last_mint: u64,
    pub total_weight: u32,
    pub total_staked: Uint128,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct VaultInfo {
    pub address: String,
    pub weight: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct VaultsResponse {
    pub vaults: Vec<VaultInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct VotersResponse {
    pub voters: Vec<(String, VoterInfo)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
