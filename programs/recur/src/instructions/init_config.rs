use anchor_lang::prelude::*;

use crate::{Config, RecurError, CONFIG_SEED, CONFIG_VERSION, MAX_BPS};

#[derive(Accounts)]
pub struct InitConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = 8 + Config::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump
    )]
    pub config: Account<'info, Config>,
    pub system_program: Program<'info, System>,
}

pub fn handle_init_config(
    ctx: Context<InitConfig>,
    fee_bps: u16,
    min_fee: u64,
    treasury: Pubkey,
) -> Result<()> {
    require!(fee_bps <= MAX_BPS, RecurError::FeeTooHigh);
    ctx.accounts.config.set_inner(Config {
        version: CONFIG_VERSION,
        bump: ctx.bumps.config,
        admin: ctx.accounts.admin.key(),
        fee_bps,
        min_fee,
        treasury,
        paused: false,
        reserved: [0; 64],
    });
    Ok(())
}
