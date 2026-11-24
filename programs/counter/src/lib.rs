//! Per-user counter: every wallet gets its own counter account.
//!
//! Concepts practiced:
//! - PDAs derived from a user's key (`seeds = [b"counter", authority]`)
//! - Storing the canonical bump and reusing it (cheaper than re-deriving)
//! - `has_one` authority checks (only the owner can change their counter)
//! - Checked arithmetic with custom errors
//! - `close = authority` to delete an account and refund its rent

use anchor_lang::prelude::*;

declare_id!("HoBHp5ZXWSwcVP7BH15K1qnn7Zn8mhgTZSEYUJLmwmeW");

#[program]
pub mod counter {
    use super::*;

    /// Creates the caller's counter PDA, starting at 0.
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let counter = &mut ctx.accounts.counter;
        counter.authority = ctx.accounts.authority.key();
        counter.count = 0;
        counter.bump = ctx.bumps.counter;
        msg!("Counter created for {}", counter.authority);
        Ok(())
    }

    pub fn increment(ctx: Context<Update>) -> Result<()> {
        let counter = &mut ctx.accounts.counter;
        counter.count = counter.count.checked_add(1).ok_or(ErrorCode::Overflow)?;
        msg!("count = {}", counter.count);
        Ok(())
    }

    pub fn decrement(ctx: Context<Update>) -> Result<()> {
        let counter = &mut ctx.accounts.counter;
        counter.count = counter.count.checked_sub(1).ok_or(ErrorCode::Underflow)?;
        msg!("count = {}", counter.count);
        Ok(())
    }

    /// Deletes the counter. Anchor zeroes the data and sends the rent back
    /// to `authority`, so the body has nothing to do.
    pub fn close(_ctx: Context<Close>) -> Result<()> {
        msg!("Counter closed");
        Ok(())
    }
}

pub const COUNTER_SEED: &[u8] = b"counter";

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        // 8-byte Anchor discriminator + the struct's size
        space = 8 + Counter::INIT_SPACE,
        seeds = [COUNTER_SEED, authority.key().as_ref()],
        bump
    )]
    pub counter: Account<'info, Counter>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Update<'info> {
    #[account(
        mut,
        seeds = [COUNTER_SEED, authority.key().as_ref()],
        bump = counter.bump,
        has_one = authority @ ErrorCode::Unauthorized,
    )]
    pub counter: Account<'info, Counter>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct Close<'info> {
    #[account(
        mut,
        seeds = [COUNTER_SEED, authority.key().as_ref()],
        bump = counter.bump,
        has_one = authority @ ErrorCode::Unauthorized,
        close = authority,
    )]
    pub counter: Account<'info, Counter>,
    #[account(mut)]
    pub authority: Signer<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct Counter {
    pub authority: Pubkey,
    pub count: u64,
    pub bump: u8,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Only the counter's authority can change it")]
    Unauthorized,
    #[msg("Counter would overflow")]
    Overflow,
    #[msg("Counter is already at zero")]
    Underflow,
}
