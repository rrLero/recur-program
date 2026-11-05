use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use subscriptions::{Plan, SUBSCRIPTIONS_ID};

use crate::{
    rail, split,
    transit::{pay_out_split, TransitPayout},
    Config, Membership, MembershipIdentity, MembershipSource, Merchant, RecurError, CONFIG_SEED,
    MEMBERSHIP_SEED, MERCHANT_SEED, PULLER_SEED, TRANSIT_SEED,
};

#[derive(Accounts)]
pub struct ActivateAgent<'info> {
    pub agent_puller: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, Config>>,
    #[account(seeds = [MERCHANT_SEED, merchant.owner.as_ref()], bump = merchant.bump)]
    pub merchant: Box<Account<'info, Merchant>>,
    #[account(seeds = [TRANSIT_SEED, merchant.owner.as_ref()], bump)]
    pub transit: SystemAccount<'info>,
    #[account(mut, token::mint = mint, token::authority = transit)]
    pub transit_ata: Box<Account<'info, TokenAccount>>,
    pub mint: Box<Account<'info, Mint>>,
    /// CHECK: owned by the rail program, deserialized and validated in the handler
    #[account(owner = SUBSCRIPTIONS_ID)]
    pub plan: UncheckedAccount<'info>,
    /// CHECK: the rail subscription PDA for this plan and member, deserialized and validated in the handler
    #[account(
        owner = SUBSCRIPTIONS_ID,
        seeds = [b"subscription", plan.key().as_ref(), member.key().as_ref()],
        bump,
        seeds::program = SUBSCRIPTIONS_ID
    )]
    pub subscription: UncheckedAccount<'info>,
    /// CHECK: the subscriber, bound by the subscription seeds and checked against its delegator in the handler
    pub member: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + Membership::INIT_SPACE,
        seeds = [MEMBERSHIP_SEED, plan.key().as_ref(), member.key().as_ref()],
        bump
    )]
    pub membership: Box<Account<'info, Membership>>,
    #[account(mut, token::mint = mint, constraint = merchant_ata.owner == merchant.owner)]
    pub merchant_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = mint, constraint = treasury_ata.owner == config.treasury)]
    pub treasury_ata: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_activate_agent(ctx: Context<ActivateAgent>) -> Result<()> {
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

    let plan = rail::read_plan(&accounts.plan)?;
    require_agent_plan(
        &plan,
        &accounts.transit.key(),
        &accounts.config.agent_puller,
    )?;
    require_keys_eq!(accounts.mint.key(), plan.data.mint);
    require_keys_eq!(accounts.merchant.owner, plan.owner);

    let subscription = rail::read_subscription(&accounts.subscription)?;
    require_keys_eq!(
        subscription.header.delegator,
        accounts.member.key(),
        RecurError::Unauthorized
    );
    require_keys_eq!(
        subscription.header.delegatee,
        accounts.plan.key(),
        RecurError::Unauthorized
    );
    let amount = plan.data.terms.amount;
    require_eq!(
        subscription.amount_pulled_in_period,
        amount,
        RecurError::PeriodNotPulled
    );
    let period_end = subscription
        .current_period_start_ts
        .checked_add(rail::period_secs(&subscription.terms)?)
        .ok_or(RecurError::MathOverflow)?;

    let membership = &accounts.membership;
    let now = Clock::get()?.unix_timestamp;
    require!(
        membership.source == MembershipSource::Paid || membership.paid_until <= now,
        RecurError::AlreadyMember
    );
    require!(
        membership.paid_until < period_end,
        RecurError::PeriodAlreadySettled
    );

    let shares = split(amount, &accounts.config, &accounts.merchant, 0, now)?;
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
    )?;

    let identity = MembershipIdentity {
        bump: ctx.bumps.membership,
        member: accounts.member.key(),
        plan: accounts.plan.key(),
        merchant: plan.owner,
        rent_payer: accounts.payer.key(),
    };
    let membership = &mut ctx.accounts.membership;
    membership.open(identity, None, 0, now)?;
    membership.paid_until = membership.paid_until.max(period_end);
    membership.source = MembershipSource::Paid;
    Ok(())
}

fn require_agent_plan(plan: &Plan, transit: &Pubkey, agent_puller: &Pubkey) -> Result<()> {
    let mut destinations = plan
        .data
        .destinations
        .iter()
        .filter(|destination| **destination != Pubkey::default());
    require!(
        destinations.next() == Some(transit) && destinations.next().is_none(),
        RecurError::NotAgentPlan
    );
    let (puller, _) = Pubkey::find_program_address(&[PULLER_SEED], &crate::ID);
    require!(
        plan.data.pullers.contains(agent_puller) && plan.data.pullers.contains(&puller),
        RecurError::NotRecurPuller
    );
    Ok(())
}
