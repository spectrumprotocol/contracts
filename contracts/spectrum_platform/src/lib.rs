// #![allow(unused_imports, non_camel_case_types, unused_variables, dead_code)]

pub mod contract;
pub mod state;

#[cfg(test)]
mod tests;

mod poll;

#[cfg(target_arch = "wasm32")]
cosmwasm_std::create_entry_points_with_migration!(contract);
