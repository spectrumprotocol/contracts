use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};
use spectrum_luna_burn_farm::model::{ConfigInfo, ExecuteMsg, QueryMsg, RewardInfoResponse, SimulateCollectResponse};
use spectrum_luna_burn_farm::state::{Hub, State, Unbonding};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(ConfigInfo), &out_dir);
    export_schema(&schema_for!(RewardInfoResponse), &out_dir);
    export_schema(&schema_for!(State), &out_dir);
    export_schema(&schema_for!(Unbonding), &out_dir);
    export_schema(&schema_for!(Hub), &out_dir);
    export_schema(&schema_for!(SimulateCollectResponse), &out_dir);
}
