use cosmwasm_std::{Decimal, HumanAddr};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::common::OrderBy;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: HumanAddr,
    pub quorum: Decimal,
    pub threshold: Decimal,
    pub voting_period: u64,
    pub effective_delay: u64,
    pub expiration_period: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum HandleMsg {
    poll_end {
        poll_id: u64,
    },
    poll_execute {
        poll_id: u64,
    },
    poll_expire {
        poll_id: u64,
    },
    poll_start {
        title: String,
        description: String,
        link: Option<String>,
        execute_msgs: Vec<ExecuteMsg>,
    },
    poll_vote {
        poll_id: u64,
        vote: VoteOption,
    },
    update_config {
        owner: Option<HumanAddr>,
        quorum: Option<Decimal>,
        threshold: Option<Decimal>,
        voting_period: Option<u64>,
        effective_delay: Option<u64>,
        expiration_period: Option<u64>,
    },
    upsert_board {
        address: HumanAddr,
        weight: u32,
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
pub enum ExecuteMsg {
    execute { contract: HumanAddr, msg: String },
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
    boards {},
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
    state {},
    voters {
        poll_id: u64,
        start_after: Option<HumanAddr>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct BoardInfo {
    pub address: HumanAddr,
    pub weight: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct BoardsResponse {
    pub boards: Vec<BoardInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct VoterInfo {
    pub vote: VoteOption,
    pub balance: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct PollInfo {
    pub id: u64,
    pub creator: HumanAddr,
    pub status: PollStatus,
    pub end_height: u64,
    pub title: String,
    pub description: String,
    pub link: Option<String>,
    pub execute_msgs: Vec<ExecuteMsg>,
    pub yes_votes: u32, // balance
    pub no_votes: u32,  // balance
    pub total_balance_at_end_poll: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct PollsResponse {
    pub polls: Vec<PollInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct StateInfo {
    pub poll_count: u64,
    pub total_weight: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct VotersResponse {
    pub voters: Vec<(HumanAddr, VoterInfo)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
