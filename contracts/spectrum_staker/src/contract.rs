use crate::state::{config_store, read_config, Config};
use cosmwasm_std::{
    log, to_binary, Api, Binary, Coin, CosmosMsg, Decimal, Env, Extern, HandleResponse, HumanAddr,
    InitResponse, MigrateResponse, MigrateResult, Querier, StdError, StdResult, Storage, Uint128,
    WasmMsg,
};
use cw20::Cw20HandleMsg;
use spectrum_protocol::mirror_farm::Cw20HookMsg;
use spectrum_protocol::staker::{HandleMsg, InitMsg, MigrateMsg, QueryMsg};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::HandleMsg as PairHandleMsg;
use terraswap::querier::{query_pair_info, query_token_balance};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    config_store(&mut deps.storage).save(&Config {
        terraswap_factory: deps.api.canonical_address(&msg.terraswap_factory)?,
    })?;
    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::bond {
            contract,
            assets,
            slippage_tolerance,
            compound_rate,
        } => bond(
            deps,
            env,
            contract,
            assets,
            slippage_tolerance,
            compound_rate,
        ),
        HandleMsg::bond_hook {
            contract,
            asset_token,
            staking_token,
            staker_addr,
            prev_staking_token_amount,
            compound_rate,
        } => bond_hook(
            deps,
            env,
            contract,
            asset_token,
            staking_token,
            staker_addr,
            prev_staking_token_amount,
            compound_rate,
        ),
    }
}

fn bond<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    contract: HumanAddr,
    assets: [Asset; 2],
    slippage_tolerance: Option<Decimal>,
    compound_rate: Option<Decimal>,
) -> StdResult<HandleResponse> {
    let config = read_config(&deps.storage)?;
    let terraswap_factory = deps.api.human_address(&config.terraswap_factory)?;

    let mut native_asset_op: Option<Asset> = None;
    let mut token_info_op: Option<(HumanAddr, Uint128)> = None;
    for asset in assets.iter() {
        match asset.info.clone() {
            AssetInfo::Token { contract_addr } => {
                token_info_op = Some((contract_addr, asset.amount))
            }
            AssetInfo::NativeToken { .. } => {
                asset.assert_sent_native_token_balance(&env)?;
                native_asset_op = Some(asset.clone())
            }
        }
    }

    // will fail if one of them is missing
    let native_asset = match native_asset_op {
        Some(v) => v,
        None => return Err(StdError::generic_err("Missing native asset")),
    };
    let (token_addr, token_amount) = match token_info_op {
        Some(v) => v,
        None => return Err(StdError::generic_err("Missing token asset")),
    };

    // query pair info to obtain pair contract address
    let asset_infos = [assets[0].info.clone(), assets[1].info.clone()];
    let terraswap_pair = query_pair_info(deps, &terraswap_factory, &asset_infos)?;

    // get current lp token amount to later compute the received amount
    let prev_staking_token_amount = query_token_balance(
        &deps,
        &terraswap_pair.liquidity_token,
        &env.contract.address,
    )?;

    // compute tax
    let tax_amount = native_asset.compute_tax(deps)?;
    let native_asset = Asset {
        amount: (native_asset.amount - tax_amount)?,
        info: native_asset.info,
    };

    // 1. Transfer token asset to staking contract
    // 2. Increase allowance of token for pair contract
    // 3. Provide liquidity
    // 4. Execute staking hook, will stake in the name of the sender
    Ok(HandleResponse {
        messages: vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_addr.clone(),
                msg: to_binary(&Cw20HandleMsg::TransferFrom {
                    owner: env.message.sender.clone(),
                    recipient: env.contract.address.clone(),
                    amount: token_amount,
                })?,
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_addr.clone(),
                msg: to_binary(&Cw20HandleMsg::IncreaseAllowance {
                    spender: terraswap_pair.contract_addr.clone(),
                    amount: token_amount,
                    expires: None,
                })?,
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: terraswap_pair.contract_addr,
                msg: to_binary(&PairHandleMsg::ProvideLiquidity {
                    assets: if let AssetInfo::NativeToken { .. } = assets[0].info.clone() {
                        [native_asset.clone(), assets[1].clone()]
                    } else {
                        [assets[0].clone(), native_asset.clone()]
                    },
                    slippage_tolerance,
                })?,
                send: vec![Coin {
                    denom: native_asset.info.to_string(),
                    amount: native_asset.amount,
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address,
                msg: to_binary(&HandleMsg::bond_hook {
                    contract,
                    asset_token: token_addr.clone(),
                    staking_token: terraswap_pair.liquidity_token,
                    staker_addr: env.message.sender,
                    prev_staking_token_amount,
                    compound_rate,
                })?,
                send: vec![],
            }),
        ],
        log: vec![
            log("action", "bond"),
            log("asset_token", token_addr.to_string()),
            log("tax_amount", tax_amount.to_string()),
        ],
        data: None,
    })
}

fn bond_hook<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    contract: HumanAddr,
    asset_token: HumanAddr,
    staking_token: HumanAddr,
    staker_addr: HumanAddr,
    prev_staking_token_amount: Uint128,
    compound_rate: Option<Decimal>,
) -> StdResult<HandleResponse> {
    // only can be called by itself
    if env.message.sender != env.contract.address {
        return Err(StdError::unauthorized());
    }

    // stake all lp tokens received, compare with staking token amount before liquidity provision was executed
    let current_staking_token_amount =
        query_token_balance(&deps, &staking_token, &env.contract.address)?;
    let amount_to_stake = (current_staking_token_amount - prev_staking_token_amount)?;

    Ok(HandleResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token,
            msg: to_binary(&Cw20HandleMsg::Send {
                amount: amount_to_stake,
                contract,
                msg: Some(to_binary(&Cw20HookMsg::bond {
                    asset_token,
                    staker_addr: Some(staker_addr),
                    compound_rate,
                })?),
            })?,
            send: vec![],
        })],
        log: vec![],
        data: None,
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    _deps: &Extern<S, A, Q>,
    _msg: QueryMsg,
) -> StdResult<Binary> {
    Err(StdError::generic_err("query not support"))
}

pub fn migrate<S: Storage, A: Api, Q: Querier>(
    _deps: &mut Extern<S, A, Q>,
    _env: Env,
    _msg: MigrateMsg,
) -> MigrateResult {
    Ok(MigrateResponse::default())
}
