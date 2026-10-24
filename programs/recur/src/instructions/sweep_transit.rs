use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::{Config, RecurError, CONFIG_SEED, TRANSIT_SEED};

#[derive(Accounts)]
pub struct SweepTransit<'info> {
    pub admin: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ RecurError::Unauthorized
    )]
    pub config: Account<'info, Config>,
    #[account(seeds = [TRANSIT_SEED], bump)]
    pub transit: SystemAccount<'info>,
    #[account(mut, token::authority = transit)]
    pub transit_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = transit_ata.mint,
        constraint = treasury_ata.owner == config.treasury
    )]
    pub treasury_ata: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_sweep_transit(ctx: Context<SweepTransit>) -> Result<()> {
    let accounts = &ctx.accounts;
    token::transfer(
        CpiContext::new_with_signer(
            accounts.token_program.key(),
            Transfer {
                from: accounts.transit_ata.to_account_info(),
                to: accounts.treasury_ata.to_account_info(),
                authority: accounts.transit.to_account_info(),
            },
            &[&[TRANSIT_SEED, &[ctx.bumps.transit]]],
        ),
        accounts.transit_ata.amount,
    )
}
