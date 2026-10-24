mod common;

use recur::RecurError;
use solana_signer::Signer;

#[test]
fn register_merchant_stores_owner_and_referral() {
    let (ctx, sub) = common::configured_sub(30 * common::USDC, 720);
    let merchant = common::fetch_merchant(&ctx, sub.merchant.pubkey());
    assert_eq!(merchant.owner, sub.merchant.pubkey());
    assert_eq!(merchant.referral_bps, 1_000);
    assert_eq!(merchant.fee_bps_override, None);
    assert_eq!(merchant.fee_free_until, 0);
}

#[test]
fn register_merchant_twice_fails() {
    let (mut ctx, sub) = common::configured_sub(30 * common::USDC, 720);
    ctx.svm.expire_blockhash();
    assert!(common::register_merchant(&mut ctx, &sub, 500).is_err());
    let merchant = common::fetch_merchant(&ctx, sub.merchant.pubkey());
    assert_eq!(merchant.referral_bps, 1_000);
}

#[test]
fn register_merchant_rejects_referral_over_100_percent() {
    let mut ctx = common::setup();
    let owner = common::funded_keypair(&mut ctx);
    let result = common::register_merchant_as(&mut ctx, &owner, 10_001);
    assert!(common::err_is(&result, RecurError::ReferralTooHigh));
}

#[test]
fn update_merchant_changes_referral() {
    let (mut ctx, sub) = common::configured_sub(30 * common::USDC, 720);
    common::update_merchant(&mut ctx, &sub.merchant, sub.merchant.pubkey(), 2_000).unwrap();
    let merchant = common::fetch_merchant(&ctx, sub.merchant.pubkey());
    assert_eq!(merchant.referral_bps, 2_000);
}

#[test]
fn update_merchant_rejects_referral_over_100_percent() {
    let (mut ctx, sub) = common::configured_sub(30 * common::USDC, 720);
    let result = common::update_merchant(&mut ctx, &sub.merchant, sub.merchant.pubkey(), 10_001);
    assert!(common::err_is(&result, RecurError::ReferralTooHigh));
}

#[test]
fn update_merchant_rejects_non_owner() {
    let (mut ctx, sub) = common::configured_sub(30 * common::USDC, 720);
    let stranger = common::funded_keypair(&mut ctx);
    let result = common::update_merchant(&mut ctx, &stranger, sub.merchant.pubkey(), 9_000);
    assert!(common::err_is(&result, RecurError::Unauthorized));
    let merchant = common::fetch_merchant(&ctx, sub.merchant.pubkey());
    assert_eq!(merchant.referral_bps, 1_000);
}

#[test]
fn set_merchant_terms_sets_override_and_window() {
    let (mut ctx, sub) = common::configured_sub(30 * common::USDC, 720);
    let admin = ctx.admin.insecure_clone();
    common::set_merchant_terms(&mut ctx, &admin, sub.merchant.pubkey(), Some(100), 5_000).unwrap();
    let merchant = common::fetch_merchant(&ctx, sub.merchant.pubkey());
    assert_eq!(merchant.fee_bps_override, Some(100));
    assert_eq!(merchant.fee_free_until, 5_000);
}

#[test]
fn set_merchant_terms_rejects_non_admin() {
    let (mut ctx, sub) = common::configured_sub(30 * common::USDC, 720);
    let result = common::set_merchant_terms(&mut ctx, &sub.merchant, sub.merchant.pubkey(), Some(0), 9_999);
    assert!(common::err_is(&result, RecurError::Unauthorized));
    let merchant = common::fetch_merchant(&ctx, sub.merchant.pubkey());
    assert_eq!(merchant.fee_bps_override, None);
}

#[test]
fn set_merchant_terms_rejects_override_over_100_percent() {
    let (mut ctx, sub) = common::configured_sub(30 * common::USDC, 720);
    let admin = ctx.admin.insecure_clone();
    let result = common::set_merchant_terms(&mut ctx, &admin, sub.merchant.pubkey(), Some(10_001), 0);
    assert!(common::err_is(&result, RecurError::FeeTooHigh));
}
