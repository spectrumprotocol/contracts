use cosmwasm_std::Order;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrderBy {
    Asc,
    Desc,
}

impl From<OrderBy> for Order {
    fn from(order: OrderBy) -> Self {
        if order == OrderBy::Asc {
            Order::Ascending
        } else {
            Order::Descending
        }
    }
}
