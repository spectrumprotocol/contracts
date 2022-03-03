pub mod anchor;
pub mod astroport_factory;
pub mod astroport_pair;
pub mod basset_vault;
pub mod basset_vault_strategy;
pub mod common;
pub mod nasset_token;
pub mod nasset_token_config_holder;
pub mod nasset_token_rewards;
pub mod psi_distributor;
pub mod querier;
pub mod terraswap;

// hom many iterations is available for loan repayment
pub const BASSET_VAULT_LOAN_REPAYMENT_MAX_RECURSION_DEEP: u8 = 10;

#[inline]
fn concat(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut k = namespace.to_vec();
    k.extend_from_slice(key);
    k
}
