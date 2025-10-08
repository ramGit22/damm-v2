use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use std::cell::RefMut;

use crate::{
    const_pda,
    constants::NUM_REWARDS,
    error::PoolError,
    event::EvtClaimReward,
    state::{pool::Pool, position::Position},
    token::transfer_from_pool,
};

#[event_cpi]
#[derive(Accounts)]
pub struct ClaimRewardCtx<'info> {
    /// CHECK: pool authority
    #[account(address = const_pda::pool_authority::ID)]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    #[account(
        mut,
        has_one = pool,
    )]
    pub position: AccountLoader<'info, Position>,

    /// The vault token account for reward token
    #[account(mut)]
    pub reward_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    // Reward mint
    pub reward_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut)]
    pub user_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The token account for nft
    #[account(
            constraint = position_nft_account.mint == position.load()?.nft_mint,
            constraint = position_nft_account.amount == 1,
            token::authority = owner
    )]
    pub position_nft_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// owner of position
    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

impl<'info> ClaimRewardCtx<'info> {
    fn validate(&self, reward_index: usize) -> Result<()> {
        let pool = self.pool.load()?;
        require!(reward_index < NUM_REWARDS, PoolError::InvalidRewardIndex);

        let reward_info = &pool.reward_infos[reward_index];
        require!(reward_info.initialized(), PoolError::RewardUninitialized);
        require!(
            reward_info.vault.eq(&self.reward_vault.key()),
            PoolError::InvalidRewardVault
        );

        Ok(())
    }
}

pub fn handle_claim_reward(
    ctx: Context<ClaimRewardCtx>,
    reward_index: u8,
    skip_reward: u8,
) -> Result<()> {
    let index: usize = reward_index
        .try_into()
        .map_err(|_| PoolError::TypeCastFailed)?;
    ctx.accounts.validate(index)?;

    let reward_vault_frozen = ctx.accounts.reward_vault.is_frozen();

    let claim_outcome = {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let mut position = ctx.accounts.position.load_mut()?;
        let current_time = Clock::get()?.unix_timestamp as u64;
        process_claim_reward(
            &mut pool,
            &mut position,
            index,
            current_time,
            reward_vault_frozen,
            skip_reward,
        )?
    };

    if let Some(total_reward) = claim_outcome {
        if total_reward > 0 {
            transfer_from_pool(
                ctx.accounts.pool_authority.to_account_info(),
                &ctx.accounts.reward_mint,
                &ctx.accounts.reward_vault,
                &ctx.accounts.user_token_account,
                &ctx.accounts.token_program,
                total_reward,
            )?;
        }

        emit_cpi!(EvtClaimReward {
            pool: ctx.accounts.pool.key(),
            position: ctx.accounts.position.key(),
            mint_reward: ctx.accounts.reward_mint.key(),
            owner: ctx.accounts.owner.key(),
            reward_index,
            total_reward,
        });
    } else {
        emit_cpi!(EvtClaimReward {
            pool: ctx.accounts.pool.key(),
            position: ctx.accounts.position.key(),
            mint_reward: ctx.accounts.reward_mint.key(),
            owner: ctx.accounts.owner.key(),
            reward_index,
            total_reward: 0,
        });
    }

    Ok(())
}

fn process_claim_reward(
    pool: &mut RefMut<'_, Pool>,
    position: &mut RefMut<'_, Position>,
    reward_index: usize,
    current_time: u64,
    reward_vault_frozen: bool,
    skip_reward: u8,
) -> Result<Option<u64>> {
    position.update_rewards(pool, current_time)?;

    if reward_vault_frozen {
        require!(
            skip_reward == 1,
            PoolError::RewardVaultFrozenSkipRequired
        );
        return Ok(None);
    }

    let total_reward = position.claim_reward(reward_index)?;
    Ok(Some(total_reward))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn setup_reward_state(pending: u64) -> (RefCell<Pool>, RefCell<Position>) {
        let mut pool = Pool::default();
        pool.reward_infos[0].initialized = 1;
        pool.reward_infos[0].reward_duration_end = u64::MAX;

        let mut position = Position::default();
        position.reward_infos[0].reward_pendings = pending;

        (RefCell::new(pool), RefCell::new(position))
    }

    #[test]
    fn skip_when_vault_frozen_preserves_pending_reward() {
        let (pool_cell, position_cell) = setup_reward_state(100);
        let mut pool_ref = pool_cell.borrow_mut();
        let mut position_ref = position_cell.borrow_mut();

        let outcome = process_claim_reward(
            &mut pool_ref,
            &mut position_ref,
            0,
            1,
            true,
            1,
        )
        .unwrap();

        assert!(outcome.is_none());
        assert_eq!(position_ref.reward_infos[0].reward_pendings, 100);
    }

    #[test]
    fn frozen_vault_without_skip_errors() {
        let (pool_cell, position_cell) = setup_reward_state(50);
        let mut pool_ref = pool_cell.borrow_mut();
        let mut position_ref = position_cell.borrow_mut();

        let err = process_claim_reward(
            &mut pool_ref,
            &mut position_ref,
            0,
            1,
            true,
            0,
        )
        .err()
        .unwrap();

        assert_eq!(err, PoolError::RewardVaultFrozenSkipRequired.into());
    }

    #[test]
    fn successful_claim_returns_amount_and_resets_pending() {
        let (pool_cell, position_cell) = setup_reward_state(77);
        let mut pool_ref = pool_cell.borrow_mut();
        let mut position_ref = position_cell.borrow_mut();

        let outcome = process_claim_reward(
            &mut pool_ref,
            &mut position_ref,
            0,
            1,
            false,
            0,
        )
        .unwrap();

        assert_eq!(outcome, Some(77));
        assert_eq!(position_ref.reward_infos[0].reward_pendings, 0);
    }
}
