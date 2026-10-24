mod common;

use common::USDC;
use recur::{fee_for, RecurError};
use solana_pubkey::Pubkey;

const PRICE: u64 = 30 * USDC;

#[test]
fn collect_splits_merchant_treasury_referrer() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::collect(&mut ctx, &sub, PRICE, Some(1_000)).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 600_000);
    assert_eq!(common::balance(&ctx, sub.referrer_ata), 2_940_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 26_460_000);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
    assert_eq!(common::balance(&ctx, sub.member_ata), 970 * USDC);
}

#[test]
fn collect_without_referrer_pays_merchant_the_rest() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::collect(&mut ctx, &sub, PRICE, None).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 600_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 29_400_000);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
}

#[test]
fn collect_twice_in_one_period_fails() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::collect(&mut ctx, &sub, PRICE, None).unwrap();
    ctx.svm.expire_blockhash();
    assert!(common::collect(&mut ctx, &sub, PRICE, None).is_err());
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 29_400_000);
    assert_eq!(common::balance(&ctx, sub.member_ata), 970 * USDC);
}

#[test]
fn collect_with_insufficient_funds_leaves_transit_empty() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::drain_member(&mut ctx, &sub);
    assert!(common::collect(&mut ctx, &sub, PRICE, None).is_err());
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 0);
}

#[test]
fn collect_rejects_plan_without_recur_puller() {
    let pullers = [
        Pubkey::new_unique(),
        Pubkey::default(),
        Pubkey::default(),
        Pubkey::default(),
    ];
    let (mut ctx, sub) = common::configured_sub_with_pullers(PRICE, 720, pullers);
    let result = common::collect(&mut ctx, &sub, PRICE, None);
    assert!(common::err_is(&result, RecurError::NotRecurPuller));
}

#[test]
fn collect_fails_when_paused() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::set_paused(&mut ctx);
    let result = common::collect(&mut ctx, &sub, PRICE, None);
    assert!(common::err_is(&result, RecurError::Paused));
}

#[test]
fn fee_uses_minimum_on_small_amounts() {
    assert_eq!(fee_for(USDC, 200, 100_000).unwrap(), 100_000);
}

#[test]
fn fee_never_exceeds_amount() {
    assert_eq!(fee_for(50_000, 200, 100_000).unwrap(), 50_000);
}

#[test]
fn collect_rejects_partial_amount() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let result = common::collect(&mut ctx, &sub, PRICE / 2, None);
    assert!(common::err_is(&result, RecurError::AmountMismatch));
    assert_eq!(common::balance(&ctx, sub.member_ata), 1_000 * USDC);
}
