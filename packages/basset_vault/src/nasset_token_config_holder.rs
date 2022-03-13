use cosmwasm_std::Addr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub governance_contract_addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Anyone { anyone_msg: AnyoneMsg },
    Governance { governance_msg: GovernanceMsg },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnyoneMsg {
    ///Anyone can set rewards_token_addr for the first time
    SetTokenRewardsContract {
        nasset_token_rewards_contract_addr: String,
    },
    AcceptGovernance {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceMsg {
    UpdateConfig {
        nasset_token_rewards_contract_addr: Option<String>,
    },
    UpdateGovernanceContract {
        gov_addr: String,
        //how long to wait for 'AcceptGovernance' transaction
        seconds_to_wait_for_accept_gov_tx: u64,
    },
}

//nasset_token use this
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub nasset_token_rewards_contract: Addr,
    pub governance_contract: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub nasset_token_rewards_addr: String,
    pub governance_contract_addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
