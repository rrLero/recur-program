use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use anchor_spl::token::{Mint, Token, TokenAccount};
use subscriptions::{
    instructions::{TransferSubscription, TransferSubscriptionInstructionArgs},
    types::TransferData,
    SUBSCRIPTIONS_ID,
};

use crate::{
    fee, rail,
    transit::{pay_out_split, TransitPayout},
    Config, Membership, MembershipSource, Merchant, RecurError, CONFIG_SEED, MEMBERSHIP_SEED,
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
    pub transit: SystemAccount<'info>,
    #[account(mut, token::mint = mint, token::authority = transit)]
    pub transit_ata: Box<Account<'info, TokenAccount>>,
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
    pub member_ata: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [MEMBERSHIP_SEED, plan.key().as_ref(), member_ata.owner.as_ref()],
        bump = membership.bump
    )]
    pub membership: Account<'info, Membership>,
    #[account(mut, token::mint = mint)]
    pub merchant_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = mint, constraint = treasury_ata.owner == config.treasury)]
    pub treasury_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = mint)]
    pub referrer_ata: Option<Box<Account<'info, TokenAccount>>>,
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
    let transit_key = accounts.transit.key();
    require!(
        plan.data.destinations.contains(&transit_key),
        RecurError::Unauthorized
    );
    let (global_transit, global_bump) = Pubkey::find_program_address(&[TRANSIT_SEED], &crate::ID);
    let is_global_transit = transit_key == global_transit;
    let transit_bump = if is_global_transit {
        [global_bump]
    } else {
        let (merchant_transit, merchant_bump) =
            Pubkey::find_program_address(&[TRANSIT_SEED, plan.owner.as_ref()], &crate::ID);
        require_keys_eq!(transit_key, merchant_transit, RecurError::Unauthorized);
        [merchant_bump]
    };

    let membership = &accounts.membership;
    let now = Clock::get()?.unix_timestamp;
    require!(
        membership.source == MembershipSource::Paid || membership.paid_until <= now,
        RecurError::PrepaidAccessActive
    );
    membership.require_referrer_account(accounts.referrer_ata.as_deref())?;

    let amount = plan.data.terms.amount;
    pull_into_transit(&ctx, amount)?;

    let subscription = rail::read_subscription(&accounts.subscription)?;
    require_keys_eq!(
        subscription.header.delegator,
        accounts.member_ata.owner,
        RecurError::Unauthorized
    );
    require_keys_eq!(
        subscription.header.delegatee,
        accounts.plan.key(),
        RecurError::Unauthorized
    );
    let period_end = subscription
        .current_period_start_ts
        .checked_add(rail::period_secs(&subscription.terms)?)
        .ok_or(RecurError::MathOverflow)?;
    let shares = fee::split(
        amount,
        &accounts.config,
        &accounts.merchant,
        membership.referral_bps,
        now,
    )?;

    let transit_signer_seeds: &[&[u8]] = if is_global_transit {
        &[TRANSIT_SEED, &transit_bump]
    } else {
        &[TRANSIT_SEED, plan.owner.as_ref(), &transit_bump]
    };
    pay_out_split(
        &TransitPayout {
            token_program: &accounts.token_program,
            transit: accounts.transit.to_account_info(),
            transit_signer_seeds,
            transit_ata: &accounts.transit_ata,
            treasury_ata: &accounts.treasury_ata,
            merchant_ata: &accounts.merchant_ata,
            referrer_ata: accounts.referrer_ata.as_deref(),
        },
        &shares,
    )?;

    let membership = &mut ctx.accounts.membership;
    membership.paid_until = membership.paid_until.max(period_end);
    membership.source = MembershipSource::Paid;
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
