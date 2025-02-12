use anchor_lang::prelude::*;

use crate::{
    activation_handler::{ActivationHandler, ActivationType},
    constants::fee::{FEE_DENOMINATOR, MEME_MIN_FEE_NUMERATOR},
    safe_math::SafeMath,
    state::{
        get_timing_constraint_by_activation_type, pool_fees::validate_fee_fraction,
        TimingConstraint,
    },
    PoolError,
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, InitSpace)]
pub struct CustomizableParams {
    /// Trading fee.
    pub trade_fee_numerator: u32,
    /// The pool start trading.
    pub activation_point: Option<u64>,
    /// Whether the pool support alpha vault
    pub has_alpha_vault: bool,
    /// Activation type
    pub activation_type: u8,
    /// Padding
    pub padding: [u8; 53],
}

// static_assertions::const_assert_eq!(CustomizableParams::INIT_SPACE, 105);

impl CustomizableParams {
    fn validation_activation(&self, timing_constraint: &TimingConstraint) -> Result<()> {
        let &TimingConstraint {
            current_point,
            min_activation_duration,
            max_activation_duration,
            pre_activation_swap_duration,
            last_join_buffer,
            ..
        } = timing_constraint;
        if self.has_alpha_vault {
            // Must specify activation point to prevent "unable" create alpha vault
            match self.activation_point {
                Some(activation_point) => {
                    require!(
                        activation_point > current_point,
                        PoolError::InvalidActivationPoint
                    );

                    // Must be within the range
                    let activation_duration = activation_point.safe_sub(current_point)?;
                    require!(
                        activation_duration >= min_activation_duration
                            && activation_duration <= max_activation_duration,
                        PoolError::InvalidActivationPoint
                    );

                    // Must have some join time
                    let activation_handler = ActivationHandler {
                        curr_point: current_point,
                        activation_point,
                        buffer_duration: pre_activation_swap_duration,
                        whitelisted_vault: Pubkey::default(),
                    };
                    let last_join_point = activation_handler.get_last_join_point()?;

                    let pre_last_join_point = last_join_point.safe_sub(last_join_buffer)?;
                    require!(
                        pre_last_join_point >= current_point,
                        PoolError::InvalidActivationPoint
                    );
                }
                None => {
                    return Err(PoolError::InvalidActivationPoint.into());
                }
            }
        } else if let Some(activation_point) = self.activation_point {
            // If no alpha vault, it's fine as long as the specified activation point is in the future, or now.
            // Prevent creation of forever untradable pool
            require!(
                activation_point >= current_point
                    && current_point.safe_add(max_activation_duration)? >= activation_point,
                PoolError::InvalidActivationPoint
            );
        }

        Ok(())
    }

    pub fn validate(self, clock: &Clock) -> Result<()> {
        let activation_type = ActivationType::try_from(self.activation_type)
            .map_err(|_| PoolError::InvalidActivationType)?;

        let timing_constraint = get_timing_constraint_by_activation_type(activation_type, clock);

        // validate fee
        self.validate_fee()?;
        // validate activation point
        self.validation_activation(&timing_constraint)?;
        Ok(())
    }

    // fn to_pool_fee(&self) -> PoolFees {
    //     PoolFees {
    //         // Pool fee start at start trading fee user specified
    //         trade_fee_numerator: self.trade_fee_numerator.into(),
    //         protocol_fee_percent: MEME_PROTOCOL_FEE_PERCENT,
    //         partner_fee_percent: 0,
    //         referral_fee_percent: 0,
    //     }
    // }

    fn validate_fee(&self) -> Result<()> {
        // 1. Fee must within the range
        let trade_fee_numerator: u64 = self.trade_fee_numerator.into();

        // avoid odd number
        require!(
            trade_fee_numerator % MEME_MIN_FEE_NUMERATOR == 0,
            PoolError::InvalidFee
        );

        // 2. Validate fee fractions.
        validate_fee_fraction(trade_fee_numerator, FEE_DENOMINATOR)?;

        Ok(())
    }
}
