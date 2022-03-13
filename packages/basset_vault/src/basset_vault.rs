use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_bignumber::{Decimal256, Uint256};
use cw20::Cw20ReceiveMsg;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    // message was too big and error returned on initialisation
    // so I cutted variables names

    // governance_contract_addr
    pub gov_addr: String,
    // community_pool_contract_addr
    pub community_addr: String,
    // nasset_token_code_id
    pub nasset_t_ci: u64,
    // nasset_token_config_holder_code_id
    pub nasset_t_ch_ci: u64,
    // nasset_token_rewards_code_id
    pub nasset_t_r_ci: u64,
    // psi_distributor_code_id
    pub psi_distr_ci: u64,
    // collateral_token_symbol
    pub collateral_ts: String,
    // basset_token_addr: String,
    pub basset_addr: String,
    // anchor_token_addr
    pub anchor_addr: String,
    // anchor_market_contract_addr
    pub a_market_addr: String,
    // anchor_overseer_contract_addr
    pub a_overseer_addr: String,
    // anchor_custody_basset_contract_addr
    pub a_custody_basset_addr: String,
    // anc_stable_swap_contract_addr
    pub anc_stable_swap_addr: String,
    // psi_stable_swap_contract_addr
    pub psi_stable_swap_addr: String,
    // aterra_token_addr
    pub aterra_addr: String,
    // psi_token_addr
    pub psi_addr: String,
    // basset_vault_strategy_contract_addr
    pub basset_vs_addr: String,
    pub stable_denom: String,
    pub claiming_rewards_delay: u64,
    ///UST value in balance should be more than loan
    ///on what portion.
    ///for example: 1.01 means 1% more than loan
    pub over_loan_balance_value: Decimal256,
    ///mean ltv that user manage by himself (advise: 60%)
    pub manual_ltv: Decimal256,
    ///fees, need to calc how much send to governance and community pools
    pub fee_rate: Decimal256,
    pub tax_rate: Decimal256,
    ///terraswap_factory_contract_addr
    pub ts_factory_addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Anyone { anyone_msg: AnyoneMsg },
    Receive(Cw20ReceiveMsg),
    Yourself { yourself_msg: YourselfMsg },
    Governance { governance_msg: GovernanceMsg },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum YourselfMsg {
    SwapAnc {},
    DisributeRewards {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnyoneMsg {
    HonestWork {},
    Rebalance {},
    // Because basset_vault always have more UST than loan,
    // then when last user will withdraw bAsset some UST remains in contract.
    // This command utilise it.
    ClaimRemainder {},
    AcceptGovernance {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceMsg {
    UpdateConfig {
        psi_distributor_addr: Option<String>,
        anchor_overseer_contract_addr: Option<String>,
        anchor_market_contract_addr: Option<String>,
        anchor_custody_basset_contract_addr: Option<String>,
        anc_stable_swap_contract_addr: Option<String>,
        psi_stable_swap_contract_addr: Option<String>,
        basset_vault_strategy_contract_addr: Option<String>,
        claiming_rewards_delay: Option<u64>,
        over_loan_balance_value: Option<Decimal256>,
    },
    UpdateGovernanceContract {
        gov_addr: String,
        //how long to wait for 'AcceptGovernance' transaction
        seconds_to_wait_for_accept_gov_tx: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Deposit {},
    Withdraw {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    Rebalance {},
    ChildContractsCodeId {},
    IsRewardsClaimable {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub governance_contract: String,
    pub nasset_token_addr: String,
    pub anchor_token_addr: String,
    pub anchor_overseer_contract_addr: String,
    pub anchor_market_contract_addr: String,
    pub anchor_custody_basset_contract_addr: String,
    pub anc_stable_swap_contract_addr: String,
    pub psi_stable_swap_contract_addr: String,
    pub basset_token_addr: String,
    pub aterra_token_addr: String,
    pub psi_token_addr: String,
    pub basset_vault_strategy_contract_addr: String,
    pub stable_denom: String,
    pub claiming_rewards_delay: u64,
    pub over_loan_balance_value: Decimal256,
    pub psi_distributor_addr: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum RebalanceResponse {
    Nothing {},
    Borrow {
        amount: Uint256,
        advised_buffer_size: Uint256,
        is_possible: bool,
    },
    Repay {
        amount: Uint256,
        advised_buffer_size: Uint256,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ChildContractsInfoResponse {
    pub nasset_token_code_id: u64,
    pub nasset_token_rewards_code_id: u64,
    pub psi_distributor_code_id: u64,
    pub collateral_token_symbol: String,
    pub community_pool_contract_addr: String,
    pub manual_ltv: Decimal256,
    pub fee_rate: Decimal256,
    pub tax_rate: Decimal256,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct IsRewardsClaimableResponse {
    pub claimable: bool,
    pub anc_amount: Decimal256,
    pub last_claiming_height: u64,
    pub current_height: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
    // pub new_over_loan_balance_value: Decimal256,
    pub new_anc_stable_swap_addr: String,
    pub new_psi_stable_swap_addr: String,
}
