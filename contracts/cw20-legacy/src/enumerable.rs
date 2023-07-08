use cosmwasm_std::{Addr, Api, CanonicalAddr, Deps, Order, StdResult};
use cw20::{AllAccountsResponse, AllAllowancesResponse, AllowanceInfo};

use crate::state::{ALLOWANCES, BALANCES};
use cw_storage_plus::Bound;

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

pub fn calc_range_start_human<'a>(
    api: &'a dyn Api,
    start_after: Option<Addr>,
    vec: &'a mut Vec<u8>,
) -> StdResult<Option<&'a [u8]>> {
    match start_after {
        Some(human) => {
            let v: Vec<_> = api.addr_canonicalize(human.as_ref())?.0.into();
            vec.extend(v);
            vec.push(0);
            Ok(Some(vec))
        }
        None => Ok(None),
    }
}

pub fn query_all_allowances(
    deps: Deps,
    owner: String,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<AllAllowancesResponse> {
    let owner_addr = deps.api.addr_canonicalize(&owner)?;
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let mut vec: Vec<u8> = vec![];
    let start =
        calc_range_start_human(deps.api, start_after.map(Addr::unchecked), &mut vec)?.map(Bound::exclusive);

    let allowances: StdResult<Vec<AllowanceInfo>> = ALLOWANCES
        .prefix(owner_addr.as_slice())
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (k, v) = item?;
            Ok(AllowanceInfo {
                spender: deps.api.addr_humanize(&CanonicalAddr::from(k))?.to_string(),
                allowance: v.allowance,
                expires: v.expires,
            })
        })
        .collect();
    Ok(AllAllowancesResponse {
        allowances: allowances?,
    })
}

pub fn query_all_accounts(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<AllAccountsResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let mut vec: Vec<u8> = vec![];
    let start =
        calc_range_start_human(deps.api, start_after.map(Addr::unchecked), &mut vec)?.map(Bound::exclusive);

    let accounts: Result<Vec<_>, _> = BALANCES
        .keys(deps.storage, start, None, Order::Ascending)
        .map(|k| {
            deps.api
                .addr_humanize(&CanonicalAddr::from(k?))
                .map(|v| v.to_string())
        })
        .take(limit)
        .collect();

    Ok(AllAccountsResponse {
        accounts: accounts?,
    })
}
