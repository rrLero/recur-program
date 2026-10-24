use anchor_lang::prelude::*;

use crate::{Config, RecurError, CONFIG_SEED, MAX_BPS};

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ RecurError::Unauthorized
    )]
    pub config: Account<'info, Config>,
}

pub fn handle_update_config(
    ctx: Context<UpdateConfig>,
    new_admin: Pubkey,
    fee_bps: u16,
    min_fee: u64,
    treasury: Pubkey,
    paused: bool,
) -> Result<()> {
    require!(fee_bps <= MAX_BPS, RecurError::FeeTooHigh);
    let config = &mut ctx.accounts.config;
    config.admin = new_admin;
    config.fee_bps = fee_bps;
    config.min_fee = min_fee;
    config.treasury = treasury;
    config.paused = paused;
    Ok(())
}
