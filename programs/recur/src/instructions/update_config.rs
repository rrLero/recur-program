use anchor_lang::prelude::*;

use crate::{is_recur_pda, Config, RecurError, CONFIG_SEED, CONFIG_VERSION, MAX_BPS};

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
    agent_puller: Pubkey,
) -> Result<()> {
    require!(fee_bps <= MAX_BPS, RecurError::FeeTooHigh);
    require!(!is_recur_pda(&agent_puller), RecurError::Unauthorized);
    let config = &mut ctx.accounts.config;
    config.admin = new_admin;
    config.fee_bps = fee_bps;
    config.min_fee = min_fee;
    config.treasury = treasury;
    config.paused = paused;
    config.agent_puller = agent_puller;
    config.version = CONFIG_VERSION;
    Ok(())
}
