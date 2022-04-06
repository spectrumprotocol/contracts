use cosmwasm_std::{Binary};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[allow(non_camel_case_types)]
pub enum QueryMsg {
    bundle {
        queries: Vec<Query>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Query {
    pub addr: String,
    pub msg: Binary,
}
