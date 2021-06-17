use cosmwasm_std::{
    Api, Binary, Env, Extern, HandleResponse, InitResponse,
    MigrateResult, Querier, StdResult, Storage,
};

use cw20_base::contract::{handle as cw20_handle, init as cw20_init, query as cw20_query, migrate as cw20_migrate};
use cw20_base::msg::{HandleMsg, InitMsg, MigrateMsg, QueryMsg};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    cw20_init(deps, env, msg)
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    cw20_handle(deps, env, msg)
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    cw20_query(deps, msg)
}

pub fn migrate<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: MigrateMsg,
) -> MigrateResult {
    cw20_migrate(deps, env, msg)
}
