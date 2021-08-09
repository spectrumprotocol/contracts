pub mod bond;
pub mod contract;
pub mod querier;
pub mod state;
pub mod compound;

#[cfg(test)]
mod tests_bond;

#[cfg(test)]
mod tests_compound;

#[cfg(test)]
mod mock_querier;


#[cfg(target_arch = "wasm32")]
cosmwasm_std::create_entry_points_with_migration!(contract);
