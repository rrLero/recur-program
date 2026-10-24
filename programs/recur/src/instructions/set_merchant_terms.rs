use anchor_lang::prelude::*;

use crate::{Config, Merchant, RecurError, CONFIG_SEED, MAX_BPS, MERCHANT_SEED};

#[derive(Accounts)]
pub struct SetMerchantTerms<'info> {
    pub admin: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ RecurError::Unauthorized
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [MERCHANT_SEED, merchant.owner.as_ref()],
        bump = merchant.bump
    )]
    pub merchant: Account<'info, Merchant>,
}

pub fn handle_set_merchant_terms(
    ctx: Context<SetMerchantTerms>,
    fee_bps_override: Option<u16>,
    fee_free_until: i64,
) -> Result<()> {
    if let Some(bps) = fee_bps_override {
        require!(bps <= MAX_BPS, RecurError::FeeTooHigh);
    }
    let merchant = &mut ctx.accounts.merchant;
    merchant.fee_bps_override = fee_bps_override;
    merchant.fee_free_until = fee_free_until;
    Ok(())
}
