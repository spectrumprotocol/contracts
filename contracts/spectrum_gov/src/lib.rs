// #![allow(unused_imports, non_camel_case_types, unused_variables, dead_code)]

pub mod contract;
pub mod state;

mod poll;
mod stake;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod mock_querier;

#[cfg(target_arch = "wasm32")]
cosmwasm_std::create_entry_points_with_migration!(contract);
