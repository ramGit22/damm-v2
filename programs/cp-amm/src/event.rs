//! Event module includes information about events of the program
use anchor_lang::prelude::*;

use crate::params::pool_fees::PoolFees;

// use crate::state::FeeCurveInfoFromDuration;

/// Close config
#[event]
pub struct EvtCloseConfig {
    /// Config pubkey
    pub config: Pubkey,
    /// admin pk
    pub admin: Pubkey,
}

/// Create config
#[event]
pub struct EvtCreateConfig {
    pub pool_fees: PoolFees,
    pub vault_config_key: Pubkey,
    pub pool_creator_authority: Pubkey,
    pub activation_type: u8,
    // pub fee_curve: FeeCurveInfoFromDuration, // TODO add this field
    pub index: u64,
    pub config: Pubkey,
}
