use anchor_lang::prelude::*;
use subscriptions::SUBSCRIPTIONS_ID;

use crate::{
    rail, Config, Membership, MembershipIdentity, MembershipSource, Merchant, RecurError,
    CONFIG_SEED, MEMBERSHIP_SEED, MERCHANT_SEED, PULLER_SEED,
};

#[derive(Accounts)]
pub struct SubscribeHook<'info> {
    pub member: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,
    /// CHECK: owned by the rail program, deserialized and discriminator-checked in the handler
    #[account(owner = SUBSCRIPTIONS_ID)]
    pub plan: UncheckedAccount<'info>,
    #[account(seeds = [MERCHANT_SEED, merchant.owner.as_ref()], bump = merchant.bump)]
    pub merchant: Account<'info, Merchant>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + Membership::INIT_SPACE,
        seeds = [MEMBERSHIP_SEED, plan.key().as_ref(), member.key().as_ref()],
        bump
    )]
    pub membership: Account<'info, Membership>,
    /// CHECK: the rail subscription PDA for this plan and member, read only by `subscription_is_over`
    #[account(
        seeds = [b"subscription", plan.key().as_ref(), member.key().as_ref()],
        bump,
        seeds::program = SUBSCRIPTIONS_ID
    )]
    pub subscription: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handle_subscribe_hook(ctx: Context<SubscribeHook>, referrer: Option<Pubkey>) -> Result<()> {
    let accounts = &ctx.accounts;
    require!(!accounts.config.paused, RecurError::Paused);

    let plan = rail::read_plan(&accounts.plan)?;
    let now = Clock::get()?.unix_timestamp;
    rail::require_plan_active(&plan, now)?;
    let (puller, _) = Pubkey::find_program_address(&[PULLER_SEED], &crate::ID);
    require!(
        plan.data.pullers.contains(&puller),
        RecurError::NotRecurPuller
    );
    require_keys_eq!(accounts.merchant.owner, plan.owner);

    let member = accounts.member.key();
    require!(
        accounts.membership.can_reopen(now),
        RecurError::AlreadyMember
    );

    let membership = &accounts.membership;
    let referral_locked = membership.created_at != 0
        && membership.source != MembershipSource::Comp
        && !rail::subscription_is_over(&accounts.subscription, now);
    let (referrer, referral_bps) = if referral_locked {
        require!(
            referrer.is_none() || referrer == membership.referrer,
            RecurError::AlreadyMember
        );
        (membership.referrer, membership.referral_bps)
    } else {
        let referral_bps = match referrer {
            Some(_) => accounts.merchant.referral_bps,
            None => 0,
        };
        (referrer, referral_bps)
    };
    let identity = MembershipIdentity {
        bump: ctx.bumps.membership,
        member,
        plan: accounts.plan.key(),
        merchant: plan.owner,
        rent_payer: accounts.payer.key(),
    };
    let membership = &mut ctx.accounts.membership;
    membership.open(identity, referrer, referral_bps, now)
}
