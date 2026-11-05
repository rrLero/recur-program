use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use subscriptions::SUBSCRIPTIONS_ID;

use crate::{
    fee, rail, Config, Membership, MembershipIdentity, MembershipSource, Merchant, RecurError,
    CONFIG_SEED, MEMBERSHIP_SEED, MERCHANT_SEED, PULLER_SEED,
};

#[derive(Accounts)]
pub struct PayUpfront<'info> {
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
    pub mint: Account<'info, Mint>,
    #[account(mut, token::mint = mint, token::authority = member)]
    pub member_ata: Account<'info, TokenAccount>,
    #[account(mut, token::mint = mint)]
    pub merchant_ata: Account<'info, TokenAccount>,
    #[account(mut, token::mint = mint, constraint = treasury_ata.owner == config.treasury)]
    pub treasury_ata: Account<'info, TokenAccount>,
    #[account(mut, token::mint = mint)]
    pub referrer_ata: Option<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_pay_upfront(ctx: Context<PayUpfront>, referrer: Option<Pubkey>) -> Result<()> {
    let accounts = &ctx.accounts;
    require!(!accounts.config.paused, RecurError::Paused);

    let plan = rail::read_plan(&accounts.plan)?;
    let now = Clock::get()?.unix_timestamp;
    rail::require_plan_active(&plan, now)?;
    let (puller, _) = Pubkey::find_program_address(&[PULLER_SEED], &crate::ID);
    require!(
        !plan.data.pullers.contains(&puller),
        RecurError::NotUpfrontPlan
    );
    require_keys_eq!(accounts.mint.key(), plan.data.mint);
    require_keys_eq!(accounts.merchant.owner, plan.owner);
    require_keys_eq!(accounts.merchant_ata.owner, plan.owner);

    let member = accounts.member.key();
    let referral_bps = accounts.merchant.referral_bps;
    let identity = MembershipIdentity {
        bump: ctx.bumps.membership,
        member,
        plan: accounts.plan.key(),
        merchant: plan.owner,
        rent_payer: accounts.payer.key(),
    };
    let membership = &mut ctx.accounts.membership;
    if membership.can_reopen(now) {
        let bps = if referrer.is_some() { referral_bps } else { 0 };
        membership.open(identity, referrer, bps, now)?;
    } else {
        require!(
            referrer.is_none() || referrer == membership.referrer,
            RecurError::AlreadyMember
        );
    }

    let accounts = &ctx.accounts;
    accounts
        .membership
        .require_referrer_account(accounts.referrer_ata.as_ref())?;

    let amount = plan.data.terms.amount;
    let shares = fee::split(
        amount,
        &accounts.config,
        &accounts.merchant,
        accounts.membership.referral_bps,
        now,
    )?;
    pay(accounts, &accounts.treasury_ata, shares.fee)?;
    if let Some(referrer_ata) = &accounts.referrer_ata {
        pay(accounts, referrer_ata, shares.referral)?;
    }
    pay(accounts, &accounts.merchant_ata, shares.merchant)?;

    let period_end = now
        .max(accounts.membership.paid_until)
        .checked_add(rail::period_secs(&plan.data.terms)?)
        .ok_or(RecurError::MathOverflow)?;
    let membership = &mut ctx.accounts.membership;
    membership.paid_until = period_end;
    membership.source = MembershipSource::Upfront;
    Ok(())
}

fn pay<'info>(
    accounts: &PayUpfront<'info>,
    to: &Account<'info, TokenAccount>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    token::transfer(
        CpiContext::new(
            accounts.token_program.key(),
            Transfer {
                from: accounts.member_ata.to_account_info(),
                to: to.to_account_info(),
                authority: accounts.member.to_account_info(),
            },
        ),
        amount,
    )
}
