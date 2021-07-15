use cosmwasm_std::{Api, CanonicalAddr, CosmosMsg, Env, Extern, HandleResponse, HandleResult, Querier, StdError, Storage, Uint128, WasmMsg, log, to_binary};

use crate::{
    state::{read_config},
};

use mirror_protocol::gov::{
    HandleMsg as MirrorGovHandleMsg,
};

// cast vote to mirror gov
pub fn cast_vote_to_mirror<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    poll_id: u64,
    amount: Uint128,
) -> HandleResult {
    let config = read_config(&deps.storage)?;

    if config.controller != CanonicalAddr::default()
        && config.controller != deps.api.canonical_address(&env.message.sender)?
    {
        return Err(StdError::unauthorized());
    }

    let mirror_gov = deps.api.human_address(&config.mirror_gov)?;

    let response = HandleResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: mirror_gov,
            msg: to_binary(&MirrorGovHandleMsg::CastVote {
                poll_id: poll_id,
                vote: mirror_protocol::gov::VoteOption::Abstain,
                amount: amount,
            })?,
            send: vec![],
        })],
        log: vec![log("action", "cast_vote_to_mirror")],
        data: None,
    };
    Ok(response)
}