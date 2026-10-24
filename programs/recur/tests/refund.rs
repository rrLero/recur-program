mod common;

use common::USDC;
use recur::RecurError;
use solana_signer::Signer;

const PRICE: u64 = 30 * USDC;

fn subscribed() -> (common::Ctx, common::Sub) {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    (ctx, sub)
}

#[test]
fn refund_pays_member_from_merchant() {
    let (mut ctx, sub) = subscribed();
    let paid_until = common::paid_until(&ctx, &sub);
    common::refund(&mut ctx, &sub, 10 * USDC, None).unwrap();
    assert_eq!(common::balance(&ctx, sub.member_ata), 980 * USDC);
    assert_eq!(
        common::balance(&ctx, sub.merchant_ata),
        29_400_000 - 10 * USDC
    );
    assert_eq!(common::paid_until(&ctx, &sub), paid_until);
}

#[test]
fn refund_can_shorten_paid_until() {
    let (mut ctx, sub) = subscribed();
    let now = common::now(&ctx);
    common::refund(&mut ctx, &sub, 0, Some(now)).unwrap();
    assert_eq!(common::paid_until(&ctx, &sub), Some(now));
    assert_eq!(common::balance(&ctx, sub.member_ata), 970 * USDC);
}

#[test]
fn refund_rejects_extending_paid_until() {
    let (mut ctx, sub) = subscribed();
    let paid_until = common::paid_until(&ctx, &sub).unwrap();
    let result = common::refund(&mut ctx, &sub, 1, Some(paid_until + 1));
    assert!(common::err_is(&result, RecurError::CannotExtend));
    assert_eq!(common::paid_until(&ctx, &sub), Some(paid_until));
    assert_eq!(common::balance(&ctx, sub.member_ata), 970 * USDC);
}

#[test]
fn refund_rejects_non_merchant() {
    let (mut ctx, sub) = subscribed();
    let stranger = common::funded_keypair(&mut ctx);
    let stranger_ata = common::token_account(&mut ctx, &sub, &stranger.pubkey());
    let mut accounts = common::refund_accounts(&sub);
    accounts.owner = stranger.pubkey();
    accounts.owner_ata = stranger_ata;
    let result = common::refund_with(&mut ctx, &stranger, accounts, 0, None);
    assert!(common::err_is(&result, RecurError::Unauthorized));
}

#[test]
fn refund_rejects_wrong_member_account() {
    let (mut ctx, sub) = subscribed();
    let stranger = common::funded_keypair(&mut ctx);
    let stranger_ata = common::token_account(&mut ctx, &sub, &stranger.pubkey());
    let mut accounts = common::refund_accounts(&sub);
    accounts.member_ata = stranger_ata;
    let merchant = sub.merchant.insecure_clone();
    let result = common::refund_with(&mut ctx, &merchant, accounts, 1, None);
    assert!(common::anchor_err_is(&result, 2015));
    assert_eq!(common::balance(&ctx, stranger_ata), 0);
}

#[test]
fn refund_works_while_paused() {
    let (mut ctx, sub) = subscribed();
    common::set_paused(&mut ctx);
    common::refund(&mut ctx, &sub, 5 * USDC, None).unwrap();
    assert_eq!(common::balance(&ctx, sub.member_ata), 975 * USDC);
}
