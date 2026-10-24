use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::{Membership, RecurError};

#[derive(Accounts)]
pub struct Refund<'info> {
    pub owner: Signer<'info>,
    #[account(mut, constraint = membership.merchant == owner.key() @ RecurError::Unauthorized)]
    pub membership: Account<'info, Membership>,
    #[account(mut, token::authority = owner)]
    pub owner_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::authority = membership.member,
        token::mint = owner_ata.mint
    )]
    pub member_ata: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_refund(ctx: Context<Refund>, amount: u64, paid_until: Option<i64>) -> Result<()> {
    if let Some(paid_until) = paid_until {
        require!(
            paid_until <= ctx.accounts.membership.paid_until,
            RecurError::CannotExtend
        );
        ctx.accounts.membership.paid_until = paid_until;
    }
    let accounts = &ctx.accounts;
    token::transfer(
        CpiContext::new(
            accounts.token_program.key(),
            Transfer {
                from: accounts.owner_ata.to_account_info(),
                to: accounts.member_ata.to_account_info(),
                authority: accounts.owner.to_account_info(),
            },
        ),
        amount,
    )
}
