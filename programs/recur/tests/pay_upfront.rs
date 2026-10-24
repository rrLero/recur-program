mod common;

use common::USDC;
use recur::{MembershipSource, RecurError};
use solana_pubkey::Pubkey;
use solana_signer::Signer;

const PRICE: u64 = 300 * USDC;
const HOURS: u64 = 8_760;
const TERM: i64 = HOURS as i64 * 3600;

#[test]
fn pay_upfront_splits_and_sets_paid_until() {
    let (mut ctx, sub) = common::configured_upfront(PRICE, HOURS);
    let now = common::now(&ctx);
    common::pay_upfront(&mut ctx, &sub, None).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 6_000_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 294_000_000);
    assert_eq!(common::balance(&ctx, sub.member_ata), 700 * USDC);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
    let membership = common::fetch_membership(&ctx, &sub);
    assert_eq!(membership.paid_until, now + TERM);
    assert_eq!(membership.source, MembershipSource::Upfront);
}

#[test]
fn pay_upfront_extends_from_current_paid_until() {
    let (mut ctx, sub) = common::configured_upfront(PRICE, HOURS);
    let now = common::now(&ctx);
    common::pay_upfront(&mut ctx, &sub, None).unwrap();
    ctx.svm.expire_blockhash();
    common::pay_upfront(&mut ctx, &sub, None).unwrap();
    assert_eq!(common::paid_until(&ctx, &sub), Some(now + 2 * TERM));
}

#[test]
fn pay_upfront_with_referrer_pays_referral() {
    let (mut ctx, sub) = common::configured_upfront(PRICE, HOURS);
    common::pay_upfront(&mut ctx, &sub, Some(sub.referrer)).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 6_000_000);
    assert_eq!(common::balance(&ctx, sub.referrer_ata), 29_400_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 264_600_000);
    assert_eq!(
        common::fetch_membership(&ctx, &sub).referrer,
        Some(sub.referrer)
    );
}

#[test]
fn pay_upfront_rejects_self_referral() {
    let (mut ctx, sub) = common::configured_upfront(PRICE, HOURS);
    let me = sub.member.pubkey();
    let ata = common::token_account(&mut ctx, &sub, &Pubkey::new_unique());
    let accounts = common::pay_upfront_accounts(&sub, Some(ata));
    let result = common::pay_upfront_with(&mut ctx, &sub, accounts, Some(me));
    assert!(common::err_is(&result, RecurError::SelfReferral));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}

#[test]
fn pay_upfront_requires_the_referrer_account() {
    let (mut ctx, sub) = common::configured_upfront(PRICE, HOURS);
    let accounts = common::pay_upfront_accounts(&sub, None);
    let result = common::pay_upfront_with(&mut ctx, &sub, accounts, Some(sub.referrer));
    assert!(common::err_is(&result, RecurError::ReferrerMismatch));
    let stranger = common::token_account(&mut ctx, &sub, &Pubkey::new_unique());
    let accounts = common::pay_upfront_accounts(&sub, Some(stranger));
    let result = common::pay_upfront_with(&mut ctx, &sub, accounts, Some(sub.referrer));
    assert!(common::err_is(&result, RecurError::ReferrerMismatch));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}

#[test]
fn pay_upfront_keeps_referrer_while_active() {
    let (mut ctx, sub) = common::configured_upfront(PRICE, HOURS);
    common::pay_upfront(&mut ctx, &sub, Some(sub.referrer)).unwrap();
    let other = Pubkey::new_unique();
    let other_ata = common::token_account(&mut ctx, &sub, &other);
    let accounts = common::pay_upfront_accounts(&sub, Some(other_ata));
    let result = common::pay_upfront_with(&mut ctx, &sub, accounts, Some(other));
    assert!(common::err_is(&result, RecurError::AlreadyMember));
    let result = common::pay_upfront(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::ReferrerMismatch));
    let accounts = common::pay_upfront_accounts(&sub, Some(sub.referrer_ata));
    common::pay_upfront_with(&mut ctx, &sub, accounts, None).unwrap();
    assert_eq!(
        common::fetch_membership(&ctx, &sub).referrer,
        Some(sub.referrer)
    );
}

#[test]
fn pay_upfront_rejects_wrong_merchant_account() {
    let (mut ctx, sub) = common::configured_upfront(PRICE, HOURS);
    let foreign = common::token_account(&mut ctx, &sub, &Pubkey::new_unique());
    let mut accounts = common::pay_upfront_accounts(&sub, None);
    accounts.merchant_ata = foreign;
    let result = common::pay_upfront_with(&mut ctx, &sub, accounts, None);
    assert!(result.is_err());
    assert_eq!(common::balance(&ctx, foreign), 0);
    assert_eq!(common::balance(&ctx, sub.member_ata), 1_000 * USDC);
}

#[test]
fn pay_upfront_rejects_foreign_treasury() {
    let (mut ctx, sub) = common::configured_upfront(PRICE, HOURS);
    let foreign = common::token_account(&mut ctx, &sub, &Pubkey::new_unique());
    let mut accounts = common::pay_upfront_accounts(&sub, None);
    accounts.treasury_ata = foreign;
    let result = common::pay_upfront_with(&mut ctx, &sub, accounts, None);
    assert!(result.is_err());
    assert_eq!(common::balance(&ctx, foreign), 0);
}

#[test]
fn pay_upfront_with_insufficient_funds_fails() {
    let (mut ctx, sub) = common::configured_upfront(PRICE, HOURS);
    common::drain_member(&mut ctx, &sub);
    let result = common::pay_upfront(&mut ctx, &sub, None);
    assert!(result.is_err());
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 0);
}

#[test]
fn pay_upfront_fails_when_paused() {
    let (mut ctx, sub) = common::configured_upfront(PRICE, HOURS);
    common::set_paused(&mut ctx);
    let result = common::pay_upfront(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::Paused));
    assert_eq!(common::balance(&ctx, sub.member_ata), 1_000 * USDC);
}

#[test]
fn pay_upfront_rejects_sunset_plan() {
    let (mut ctx, sub) = common::configured_upfront(PRICE, HOURS);
    common::set_plan(&mut ctx, &sub, |plan| plan.status = 0);
    let result = common::pay_upfront(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::PlanNotActive));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}

#[test]
fn pay_upfront_rejects_recurring_plan() {
    let (mut ctx, sub) = common::configured_sub(PRICE, HOURS);
    let result = common::pay_upfront(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::NotUpfrontPlan));
    assert_eq!(common::balance(&ctx, sub.member_ata), 1_000 * USDC);
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}

#[test]
fn pay_upfront_rejects_wrong_mint() {
    let (mut ctx, sub) = common::configured_upfront(PRICE, HOURS);
    let mint = common::new_mint(&mut ctx);
    let treasury = common::fetch_config(&ctx).treasury;
    let mut accounts = common::pay_upfront_accounts(&sub, None);
    accounts.mint = mint;
    accounts.member_ata = common::ata(&mut ctx, &mint, &sub.member.pubkey());
    accounts.merchant_ata = common::ata(&mut ctx, &mint, &sub.merchant.pubkey());
    accounts.treasury_ata = common::ata(&mut ctx, &mint, &treasury);
    let result = common::pay_upfront_with(&mut ctx, &sub, accounts, None);
    assert!(common::anchor_err_is(&result, 2502));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
    assert_eq!(common::balance(&ctx, sub.member_ata), 1_000 * USDC);
}
