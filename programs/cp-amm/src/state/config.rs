use crate::constants::activation::*;
use crate::constants::fee::FEE_DENOMINATOR;
use crate::safe_math::SafeMath;
use crate::utils_math::safe_mul_div_cast_u64;
use crate::{activation_handler::ActivationType, alpha_vault::alpha_vault};
use anchor_lang::prelude::*;

use super::pool_fees::{PartnerInfo, PoolFees};

/// Encodes all results of swapping
#[derive(Debug, PartialEq)]
pub struct FeeOnAmountResult {
    pub amount: u64,
    pub lp_fee: u64,
    pub protocol_fee: u64,
    pub partner_fee: u64,
    pub referral_fee: u64,
}

#[zero_copy]
/// Information regarding fee charges
/// trading_fee = amount * trade_fee_numerator / denominator
/// protocol_fee = trading_fee * protocol_fee_percentage / 100
/// referral_fee = protocol_fee * referral_percentage / 100
/// partner_fee = (protocol_fee - referral_fee) * partner_fee_percentage / denominator
#[derive(Debug, AnchorSerialize, AnchorDeserialize, InitSpace, Default)]
pub struct PoolFeesStruct {
    /// Trade fees are extra token amounts that are held inside the token
    /// accounts during a trade, making the value of liquidity tokens rise.
    /// Trade fee numerator
    pub trade_fee_numerator: u64,

    /// Protocol trading fees are extra token amounts that are held inside the token
    /// accounts during a trade, with the equivalent in pool tokens minted to
    /// the protocol of the program.
    /// Protocol trade fee numerator
    pub protocol_fee_percent: u8,
    /// partner fee
    pub partner_fee_percent: u8,
    /// referral fee
    pub referral_fee_percent: u8,
    /// padding
    pub padding_0: [u8; 5],
    /// padding
    pub padding_1: [u64; 2],
}

impl PoolFeesStruct {
    fn from_pool_fees(pool_fees: &PoolFees) -> Self {
        let &PoolFees {
            trade_fee_numerator,
            protocol_fee_percent,
            partner_fee_percent,
            referral_fee_percent,
        } = pool_fees;
        Self {
            trade_fee_numerator,
            protocol_fee_percent,
            partner_fee_percent,
            referral_fee_percent,
            ..Default::default()
        }
    }

    pub fn get_fee_on_amount(&self, amount: u64, is_referral: bool) -> Result<FeeOnAmountResult> {
        let lp_fee: u64 = safe_mul_div_cast_u64(amount, self.trade_fee_numerator, FEE_DENOMINATOR)?;
        // update amount
        let amount = amount.safe_sub(lp_fee)?;

        let protocol_fee = safe_mul_div_cast_u64(lp_fee, self.protocol_fee_percent.into(), 100)?;
        // update lp fee
        let lp_fee = lp_fee.safe_sub(protocol_fee)?;

        let referral_fee = if is_referral {
            safe_mul_div_cast_u64(protocol_fee, self.referral_fee_percent.into(), 100)?
        } else {
            0
        };

        let protocol_fee_after_referral_fee = protocol_fee.safe_sub(referral_fee)?;
        let partner_fee = safe_mul_div_cast_u64(
            protocol_fee_after_referral_fee,
            self.partner_fee_percent.into(),
            100,
        )?;

        let protocol_fee = protocol_fee_after_referral_fee.safe_sub(partner_fee)?;

        Ok(FeeOnAmountResult {
            amount,
            lp_fee,
            protocol_fee,
            partner_fee,
            referral_fee,
        })
    }
}

#[account(zero_copy)]
#[derive(InitSpace, Debug)]
pub struct Config {
    /// Vault config key
    pub vault_config_key: Pubkey,
    /// Only pool_creator_authority can use the current config to initialize new pool. When it's Pubkey::default, it's a public config.
    pub pool_creator_authority: Pubkey,
    /// Pool fee
    pub pool_fees: PoolFeesStruct,
    /// Activation type
    pub activation_type: u8,
    /// Collect fee mode
    pub collect_fee_mode: u8,
    /// padding 0
    pub _padding_0: [u8; 6],
    /// sqrt min price
    pub sqrt_min_price: u128,
    /// sqrt max price
    pub sqrt_max_price: u128,
    /// Fee curve point
    /// Padding for further use
    pub _padding_1: [u64; 10],
}

pub struct BootstrappingConfig {
    pub activation_point: u64,
    pub vault_config_key: Pubkey,
    pub activation_type: u8,
}

pub struct TimingConstraint {
    pub current_point: u64,
    pub min_activation_duration: u64,
    pub max_activation_duration: u64,
    pub pre_activation_swap_duration: u64,
    pub last_join_buffer: u64,
    pub max_fee_curve_duration: u64,
    pub max_high_tax_duration: u64,
}

pub fn get_timing_constraint_by_activation_type(
    activation_type: ActivationType,
    clock: &Clock,
) -> TimingConstraint {
    match activation_type {
        ActivationType::Slot => TimingConstraint {
            current_point: clock.slot,
            min_activation_duration: SLOT_BUFFER,
            max_activation_duration: MAX_ACTIVATION_SLOT_DURATION,
            pre_activation_swap_duration: SLOT_BUFFER,
            last_join_buffer: FIVE_MINUTES_SLOT_BUFFER,
            max_fee_curve_duration: MAX_FEE_CURVE_SLOT_DURATION,
            max_high_tax_duration: MAX_HIGH_TAX_SLOT_DURATION,
        },
        ActivationType::Timestamp => TimingConstraint {
            current_point: clock.unix_timestamp as u64,
            min_activation_duration: TIME_BUFFER,
            max_activation_duration: MAX_ACTIVATION_TIME_DURATION,
            pre_activation_swap_duration: TIME_BUFFER,
            last_join_buffer: FIVE_MINUTES_TIME_BUFFER,
            max_fee_curve_duration: MAX_FEE_CURVE_TIME_DURATION,
            max_high_tax_duration: MAX_HIGH_TAX_TIME_DURATION,
        },
    }
}

impl Config {
    pub fn init(
        &mut self,
        pool_fees: &PoolFees,
        vault_config_key: Pubkey,
        pool_creator_authority: Pubkey,
        activation_type: u8,
        sqrt_min_price: u128,
        sqrt_max_price: u128,
        collect_fee_mode: u8,
    ) {
        self.pool_fees = PoolFeesStruct::from_pool_fees(pool_fees);
        self.vault_config_key = vault_config_key;
        self.pool_creator_authority = pool_creator_authority;
        self.activation_type = activation_type;
        self.sqrt_min_price = sqrt_min_price;
        self.sqrt_max_price = sqrt_max_price;
        self.collect_fee_mode = collect_fee_mode;
    }

    pub fn to_bootstrapping_config(&self, activation_point: u64) -> BootstrappingConfig {
        BootstrappingConfig {
            activation_point,
            vault_config_key: self.vault_config_key,
            activation_type: self.activation_type,
        }
    }

    pub fn get_partner_info(&self) -> PartnerInfo {
        PartnerInfo {
            partner_authority: self.pool_creator_authority,
            fee_percent: self.pool_fees.partner_fee_percent,
            ..Default::default()
        }
    }

    pub fn get_whitelisted_alpha_vault(&self, pool: Pubkey) -> Pubkey {
        if self.vault_config_key.eq(&Pubkey::default()) {
            Pubkey::default()
        } else {
            alpha_vault::derive_vault_pubkey(self.vault_config_key, pool.key())
        }
    }
}
