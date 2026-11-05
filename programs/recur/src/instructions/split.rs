use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::{
    per_call_split,
    transit::{pay_out_split, TransitPayout},
    Config, Merchant, RecurError, CONFIG_SEED, MERCHANT_SEED, TRANSIT_SEED,
};

#[derive(Accounts)]
pub struct Split<'info> {
    pub agent_puller: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, Config>>,
    #[account(seeds = [MERCHANT_SEED, merchant.owner.as_ref()], bump = merchant.bump)]
    pub merchant: Box<Account<'info, Merchant>>,
    #[account(seeds = [TRANSIT_SEED, merchant.owner.as_ref()], bump)]
    pub transit: SystemAccount<'info>,
    #[account(mut, token::mint = mint, token::authority = transit)]
    pub transit_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = mint, constraint = merchant_ata.owner == merchant.owner)]
    pub merchant_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = mint, constraint = treasury_ata.owner == config.treasury)]
    pub treasury_ata: Box<Account<'info, TokenAccount>>,
    pub mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_split(ctx: Context<Split>, amount: u64) -> Result<()> {
    let accounts = &ctx.accounts;
    require!(!accounts.config.paused, RecurError::Paused);
    require_keys_neq!(
        accounts.config.agent_puller,
        Pubkey::default(),
        RecurError::AgentPullerNotSet
    );
    require_keys_eq!(
        accounts.agent_puller.key(),
        accounts.config.agent_puller,
        RecurError::Unauthorized
    );
    require!(
        amount > 0 && amount <= accounts.transit_ata.amount,
        RecurError::InsufficientTransit
    );

    let now = Clock::get()?.unix_timestamp;
    let shares = per_call_split(amount, &accounts.config, &accounts.merchant, now)?;
    let merchant_owner = accounts.merchant.owner;
    let transit_bump = [ctx.bumps.transit];
    let transit_signer_seeds: &[&[u8]] = &[TRANSIT_SEED, merchant_owner.as_ref(), &transit_bump];
    pay_out_split(
        &TransitPayout {
            token_program: &accounts.token_program,
            transit: accounts.transit.to_account_info(),
            transit_signer_seeds,
            transit_ata: &accounts.transit_ata,
            treasury_ata: &accounts.treasury_ata,
            merchant_ata: &accounts.merchant_ata,
            referrer_ata: None,
        },
        &shares,
    )
}
