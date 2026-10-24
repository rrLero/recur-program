use anchor_lang::prelude::*;
use subscriptions::SUBSCRIPTIONS_ID;

use crate::{
    rail, Membership, MembershipIdentity, MembershipSource, RecurError, MEMBERSHIP_SEED,
};

#[derive(Accounts)]
#[instruction(member: Pubkey)]
pub struct GrantComp<'info> {
    pub owner: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: owned by the rail program, deserialized and discriminator-checked in the handler
    #[account(owner = SUBSCRIPTIONS_ID)]
    pub plan: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + Membership::INIT_SPACE,
        seeds = [MEMBERSHIP_SEED, plan.key().as_ref(), member.as_ref()],
        bump
    )]
    pub membership: Account<'info, Membership>,
    pub system_program: Program<'info, System>,
}

pub fn handle_grant_comp(ctx: Context<GrantComp>, member: Pubkey, until: i64) -> Result<()> {
    let accounts = &ctx.accounts;
    let plan = rail::read_plan(&accounts.plan)?;
    require_keys_eq!(plan.owner, accounts.owner.key(), RecurError::Unauthorized);
    let now = Clock::get()?.unix_timestamp;
    require!(until > now, RecurError::InvalidCompEnd);
    require!(
        accounts.membership.can_reopen(now),
        RecurError::AlreadyMember
    );

    let identity = MembershipIdentity {
        bump: ctx.bumps.membership,
        member,
        plan: accounts.plan.key(),
        merchant: plan.owner,
        rent_payer: accounts.payer.key(),
    };
    let membership = &mut ctx.accounts.membership;
    if membership.created_at == 0 {
        membership.open(identity, None, 0, now)?;
    }
    membership.paid_until = until;
    membership.source = MembershipSource::Comp;
    Ok(())
}

#[derive(Accounts)]
pub struct RevokeComp<'info> {
    pub owner: Signer<'info>,
    /// CHECK: owned by the rail program, deserialized and discriminator-checked in the handler
    #[account(owner = SUBSCRIPTIONS_ID)]
    pub plan: UncheckedAccount<'info>,
    #[account(mut, constraint = membership.plan == plan.key())]
    pub membership: Account<'info, Membership>,
}

pub fn handle_revoke_comp(ctx: Context<RevokeComp>) -> Result<()> {
    let accounts = &ctx.accounts;
    let plan = rail::read_plan(&accounts.plan)?;
    require_keys_eq!(plan.owner, accounts.owner.key(), RecurError::Unauthorized);
    require!(
        accounts.membership.source == MembershipSource::Comp,
        RecurError::NotComp
    );
    let now = Clock::get()?.unix_timestamp;
    let membership = &mut ctx.accounts.membership;
    membership.paid_until = membership.paid_until.min(now);
    Ok(())
}
