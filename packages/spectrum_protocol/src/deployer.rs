use cosmwasm_std::Binary;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: String,
    pub operator: String,
    pub time_lock: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteMsg {
    add_contract {
        contract_addr: String,
        code_id: u64,
    },
    update_contract {
        contract_addr: String,
        add_code_id: Option<u64>,
        remove_code_ids: Option<Vec<u64>>,
    },
    migrate {
        contract_addr: String,
        code_id: u64,
        msg: Binary,
    },
    update_config {
        owner: Option<String>,
        operator: Option<String>,
        time_lock: Option<u64>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    config {},
    contract {
        contract_addr: String,
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct CodeInfo {
    pub code_id: u64,
    pub created_time: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct ContractInfo {
    pub codes: Vec<CodeInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
