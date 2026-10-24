use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use subscriptions::{
    instructions::{TransferSubscription, TransferSubscriptionInstructionArgs},
    types::TransferData,
    SUBSCRIPTIONS_ID,
};

use crate::{
    rail, split, Config, Membership, Merchant, RecurError, CONFIG_SEED, MEMBERSHIP_SEED,
    MERCHANT_SEED, PULLER_SEED, TRANSIT_SEED,
};

#[derive(Accounts)]
pub struct Collect<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(seeds = [MERCHANT_SEED, merchant.owner.as_ref()], bump = merchant.bump)]
    pub merchant: Account<'info, Merchant>,
    #[account(seeds = [PULLER_SEED], bump)]
    pub puller: SystemAccount<'info>,
    #[account(seeds = [TRANSIT_SEED], bump)]
    pub transit: SystemAccount<'info>,
    #[account(mut, token::mint = mint, token::authority = transit)]
    pub transit_ata: Account<'info, TokenAccount>,
    pub mint: Account<'info, Mint>,
    /// CHECK: owned by the rail program, deserialized and validated in the handler
    #[account(owner = SUBSCRIPTIONS_ID)]
    pub plan: UncheckedAccount<'info>,
    /// CHECK: owned by the rail program, which validates it against the plan
    #[account(mut, owner = SUBSCRIPTIONS_ID)]
    pub subscription: UncheckedAccount<'info>,
    /// CHECK: owned by the rail program, which validates it as the member's delegate
    #[account(owner = SUBSCRIPTIONS_ID)]
    pub subscription_authority: UncheckedAccount<'info>,
    #[account(mut, token::mint = mint)]
    pub member_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [MEMBERSHIP_SEED, plan.key().as_ref(), member_ata.owner.as_ref()],
        bump = membership.bump
    )]
    pub membership: Account<'info, Membership>,
    #[account(mut, token::mint = mint)]
    pub merchant_ata: Account<'info, TokenAccount>,
    #[account(mut, token::mint = mint, constraint = treasury_ata.owner == config.treasury)]
    pub treasury_ata: Account<'info, TokenAccount>,
    #[account(mut, token::mint = mint)]
    pub referrer_ata: Option<Account<'info, TokenAccount>>,
    /// CHECK: the rail's event authority PDA, validated by the rail
    pub event_authority: UncheckedAccount<'info>,
    /// CHECK: address-constrained to the rail program
    #[account(address = SUBSCRIPTIONS_ID)]
    pub rail_program: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_collect(ctx: Context<Collect>) -> Result<()> {
    let accounts = &ctx.accounts;
    require!(!accounts.config.paused, RecurError::Paused);

    let plan = rail::read_plan(&accounts.plan)?;
    require!(
        plan.data.pullers.contains(&accounts.puller.key()),
        RecurError::NotRecurPuller
    );
    require_keys_eq!(accounts.mint.key(), plan.data.mint);
    require_keys_eq!(accounts.merchant.owner, plan.owner);
    require_keys_eq!(accounts.merchant_ata.owner, plan.owner);

    let membership = &accounts.membership;
    match (membership.referrer, &accounts.referrer_ata) {
        (Some(referrer), Some(referrer_ata)) => {
            require_keys_eq!(referrer_ata.owner, referrer, RecurError::ReferrerMismatch)
        }
        (None, None) => {}
        _ => return err!(RecurError::ReferrerMismatch),
    }

    let amount = plan.data.terms.amount;
    pull_into_transit(&ctx, amount)?;

    let subscription = rail::read_subscription(&accounts.subscription)?;
    let period_end = subscription
        .current_period_start_ts
        .checked_add(rail::period_secs(&subscription.terms)?)
        .ok_or(RecurError::MathOverflow)?;
    let now = Clock::get()?.unix_timestamp;
    let shares = split(
        amount,
        &accounts.config,
        &accounts.merchant,
        membership.referral_bps,
        now,
    )?;

    pay_out(&ctx, &accounts.treasury_ata, shares.fee)?;
    if let Some(referrer_ata) = &accounts.referrer_ata {
        pay_out(&ctx, referrer_ata, shares.referral)?;
    }
    pay_out(&ctx, &accounts.merchant_ata, shares.merchant)?;

    let membership = &mut ctx.accounts.membership;
    membership.paid_until = membership.paid_until.max(period_end);
    Ok(())
}

fn pull_into_transit(ctx: &Context<Collect>, amount: u64) -> Result<()> {
    let accounts = &ctx.accounts;
    let ix = TransferSubscription {
        subscription_pda: accounts.subscription.key(),
        plan_pda: accounts.plan.key(),
        subscription_authority: accounts.subscription_authority.key(),
        delegator_ata: accounts.member_ata.key(),
        receiver_ata: accounts.transit_ata.key(),
        caller: accounts.puller.key(),
        token_mint: accounts.mint.key(),
        token_program: accounts.token_program.key(),
        event_authority: accounts.event_authority.key(),
        self_program: SUBSCRIPTIONS_ID,
    }
    .instruction(TransferSubscriptionInstructionArgs {
        transfer_data: TransferData {
            amount,
            delegator: accounts.member_ata.owner,
            mint: accounts.mint.key(),
        },
    });
    invoke_signed(
        &ix,
        &[
            accounts.subscription.to_account_info(),
            accounts.plan.to_account_info(),
            accounts.subscription_authority.to_account_info(),
            accounts.member_ata.to_account_info(),
            accounts.transit_ata.to_account_info(),
            accounts.puller.to_account_info(),
            accounts.mint.to_account_info(),
            accounts.token_program.to_account_info(),
            accounts.event_authority.to_account_info(),
            accounts.rail_program.to_account_info(),
        ],
        &[&[PULLER_SEED, &[ctx.bumps.puller]]],
    )?;
    Ok(())
}

fn pay_out<'info>(
    ctx: &Context<'_, Collect<'info>>,
    to: &Account<'info, TokenAccount>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let accounts = &ctx.accounts;
    token::transfer(
        CpiContext::new_with_signer(
            accounts.token_program.key(),
            Transfer {
                from: accounts.transit_ata.to_account_info(),
                to: to.to_account_info(),
                authority: accounts.transit.to_account_info(),
            },
            &[&[TRANSIT_SEED, &[ctx.bumps.transit]]],
        ),
        amount,
    )
}
