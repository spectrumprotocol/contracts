use cosmwasm_std::{Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::enumerations::VoteOption;
use crate::common::ExecutionMsg;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub contract_config: ContractConfigInitMsg,
    pub poll_config: PollConfigInitMsg,
    pub staking_config: StakingConfigInitMsg,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ContractConfigInitMsg {
    pub governance_token: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PollConfigInitMsg {
    pub quorum: Decimal,
    pub threshold: Decimal,
    pub voting_period: u64,
    pub execution_delay_period: u64,
    pub proposal_deposit: Uint128,
    pub snapshot_period: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StakingConfigInitMsg {
    pub distributor: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    UpdateStakingConfig {
        distributor: Option<String>,
    },
    UpdatePollConfig {
        quorum: Option<Decimal>,
        threshold: Option<Decimal>,
        voting_period: Option<u64>,
        execution_delay_period: Option<u64>,
        proposal_deposit: Option<Uint128>,
        snapshot_period: Option<u64>,
    },
    StakeGovernanceTokenHook {
        staker: String,
        amount: Uint128,
    },
    UnstakeGovernanceToken { amount: Option<Uint128> },
    UnstakeGovernanceTokenHook {
        staker: String,
        amount: Option<Uint128>,
    },
    CastVote {
        poll_id: u64,
        vote: VoteOption,
        amount: Uint128,
    },
    SnapshotPoll { poll_id: u64 },
    EndPoll { poll_id: u64 },
    ExecutePoll { poll_id: u64 },
    RunExecution { executions: Vec<ExecutionMsg> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    StakeGovernanceToken {},
    CreatePoll {
        title: String,
        description: String,
        link: Option<String>,
        executions: Vec<ExecutionMsg>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
    pub staking_config: StakingConfigInitMsg,
}
