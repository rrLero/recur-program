use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::{RecurError, Split};

pub struct TransitPayout<'a, 'info> {
    pub token_program: &'a Program<'info, Token>,
    pub transit: AccountInfo<'info>,
    pub transit_signer_seeds: &'a [&'a [u8]],
    pub transit_ata: &'a Account<'info, TokenAccount>,
    pub treasury_ata: &'a Account<'info, TokenAccount>,
    pub merchant_ata: &'a Account<'info, TokenAccount>,
    pub referrer_ata: Option<&'a Account<'info, TokenAccount>>,
}

pub fn pay_out_split(payout: &TransitPayout, shares: &Split) -> Result<()> {
    pay_out(payout, payout.treasury_ata, shares.fee)?;
    match payout.referrer_ata {
        Some(referrer_ata) => pay_out(payout, referrer_ata, shares.referral)?,
        None => require_eq!(shares.referral, 0, RecurError::ReferrerMismatch),
    }
    pay_out(payout, payout.merchant_ata, shares.merchant)
}

fn pay_out<'info>(
    payout: &TransitPayout<'_, 'info>,
    to: &Account<'info, TokenAccount>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    token::transfer(
        CpiContext::new_with_signer(
            payout.token_program.key(),
            Transfer {
                from: payout.transit_ata.to_account_info(),
                to: to.to_account_info(),
                authority: payout.transit.clone(),
            },
            &[payout.transit_signer_seeds],
        ),
        amount,
    )
}
