use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128};
use crate::state::{pool_info_read, read_config};

pub fn bond(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender_addr: String,
    dp_token: String,
    amount: Uint128
) -> StdResult<Response> {
    let dp_token_raw = deps.api.addr_canonicalize(&dp_token)?; // terra1zsaswh926ey8qa5x4vj93kzzlfnef0pstuca0y

    let pool_info = pool_info_read(deps.storage).load(dp_token_raw.as_slice())?;

    // only dp token contract can execute this message
    if dp_token_raw != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;

    let lp_balance = query_pylon_pool_balance(
        deps.as_ref(),
        &config.pylon_staking,
        &env.contract.address,
    )?;

    // bond_internal(
    //     deps.branch(),
    //     env,
    //     staker_addr_raw,
    //     asset_token_raw.clone(),
    //     amount_to_auto,
    //     amount_to_stake,
    //     lp_balance,
    //     &config,
    //     false,
    // )?;
    //
    // stake_token(
    //     deps.api,
    //     config.pylon_staking,
    //     pool_info.staking_token,
    //     asset_token_raw,
    //     amount,
    // )
}
