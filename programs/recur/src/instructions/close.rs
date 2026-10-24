use anchor_lang::prelude::*;
use subscriptions::SUBSCRIPTIONS_ID;

use crate::{rail, Membership, RecurError};

#[derive(Accounts)]
pub struct Close<'info> {
    #[account(mut, close = rent_payer)]
    pub membership: Account<'info, Membership>,
    #[account(mut, address = membership.rent_payer)]
    pub rent_payer: SystemAccount<'info>,
    /// CHECK: the rail subscription PDA for this plan and member, read only by `subscription_is_over`
    #[account(
        seeds = [b"subscription", membership.plan.as_ref(), membership.member.as_ref()],
        bump,
        seeds::program = SUBSCRIPTIONS_ID
    )]
    pub subscription: UncheckedAccount<'info>,
}

pub fn handle_close(ctx: Context<Close>) -> Result<()> {
    let accounts = &ctx.accounts;
    let now = Clock::get()?.unix_timestamp;
    require!(
        now >= accounts.membership.paid_until,
        RecurError::MembershipActive
    );
    require!(
        rail::subscription_is_over(&accounts.subscription, now),
        RecurError::SubscriptionLive
    );
    Ok(())
}
