use anchor_lang::prelude::*;

use crate::{Merchant, RecurError, MAX_BPS, MERCHANT_SEED, MERCHANT_VERSION};

#[derive(Accounts)]
pub struct RegisterMerchant<'info> {
    pub owner: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + Merchant::INIT_SPACE,
        seeds = [MERCHANT_SEED, owner.key().as_ref()],
        bump
    )]
    pub merchant: Account<'info, Merchant>,
    pub system_program: Program<'info, System>,
}

pub fn handle_register_merchant(ctx: Context<RegisterMerchant>, referral_bps: u16) -> Result<()> {
    require!(referral_bps <= MAX_BPS, RecurError::ReferralTooHigh);
    ctx.accounts.merchant.set_inner(Merchant {
        version: MERCHANT_VERSION,
        bump: ctx.bumps.merchant,
        owner: ctx.accounts.owner.key(),
        referral_bps,
        fee_bps_override: None,
        fee_free_until: 0,
        reserved: [0; 64],
    });
    Ok(())
}

#[derive(Accounts)]
pub struct UpdateMerchant<'info> {
    pub owner: Signer<'info>,
    #[account(
        mut,
        seeds = [MERCHANT_SEED, merchant.owner.as_ref()],
        bump = merchant.bump,
        has_one = owner @ RecurError::Unauthorized
    )]
    pub merchant: Account<'info, Merchant>,
}

pub fn handle_update_merchant(ctx: Context<UpdateMerchant>, referral_bps: u16) -> Result<()> {
    require!(referral_bps <= MAX_BPS, RecurError::ReferralTooHigh);
    ctx.accounts.merchant.referral_bps = referral_bps;
    Ok(())
}
