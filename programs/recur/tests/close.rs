mod common;

use common::USDC;
use recur::RecurError;
use solana_pubkey::Pubkey;
use solana_signer::Signer;

const PRICE: u64 = 30 * USDC;
const PERIOD: i64 = 720 * 3600;
const DAY: i64 = 86_400;

#[test]
fn close_returns_rent_to_rent_payer_after_cancel() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    common::cancel_subscription_now(&mut ctx, &sub).unwrap();
    common::warp(&mut ctx, PERIOD + DAY);
    let membership = common::ix::membership_pda(&sub.plan, &sub.member.pubkey());
    let rent = ctx.svm.get_account(&membership).unwrap().lamports;
    let before = ctx.svm.get_balance(&sub.member.pubkey()).unwrap();
    common::close(&mut ctx, &sub).unwrap();
    assert_eq!(
        ctx.svm.get_balance(&sub.member.pubkey()).unwrap(),
        before + rent
    );
    assert!(ctx.svm.get_account(&membership).is_none());
}

#[test]
fn close_comp_membership_after_expiry() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    common::grant_comp(&mut ctx, &sub, now + DAY).unwrap();
    common::warp(&mut ctx, DAY);
    common::close(&mut ctx, &sub).unwrap();
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}

#[test]
fn close_refuses_while_active() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    common::cancel_subscription_now(&mut ctx, &sub).unwrap();
    let result = common::close(&mut ctx, &sub);
    assert!(common::err_is(&result, RecurError::MembershipActive));
    assert!(common::try_fetch_membership(&ctx, &sub).is_some());
}

#[test]
fn close_refuses_while_rail_subscription_live() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    common::warp(&mut ctx, PERIOD + DAY);
    let result = common::close(&mut ctx, &sub);
    assert!(common::err_is(&result, RecurError::SubscriptionLive));
    assert!(common::try_fetch_membership(&ctx, &sub).is_some());
}

#[test]
fn close_rejects_wrong_rent_payer() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    common::grant_comp(&mut ctx, &sub, now + DAY).unwrap();
    common::warp(&mut ctx, DAY);
    let result = common::close_with(&mut ctx, common::close_accounts(&sub, Pubkey::new_unique()));
    assert!(common::anchor_err_is(&result, 2012));
    assert!(common::try_fetch_membership(&ctx, &sub).is_some());
}

#[test]
fn subscribe_after_close_reopens_fresh() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    common::grant_comp(&mut ctx, &sub, now + DAY).unwrap();
    let first = common::fetch_membership(&ctx, &sub);
    assert_eq!(first.rent_payer, sub.merchant.pubkey());
    common::warp(&mut ctx, DAY);
    common::close(&mut ctx, &sub).unwrap();
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let second = common::fetch_membership(&ctx, &sub);
    assert_eq!(second.rent_payer, sub.member.pubkey());
    assert!(second.created_at > first.created_at);
}

#[test]
fn close_rejects_foreign_subscription_account() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    common::warp(&mut ctx, PERIOD + DAY);
    let mut accounts = common::close_accounts(&sub, sub.member.pubkey());
    accounts.subscription = Pubkey::new_unique();
    let result = common::close_with(&mut ctx, accounts);
    assert!(common::anchor_err_is(&result, 2006));
    assert!(common::try_fetch_membership(&ctx, &sub).is_some());
}

#[test]
fn close_works_while_paused() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    common::grant_comp(&mut ctx, &sub, now + DAY).unwrap();
    common::warp(&mut ctx, DAY);
    common::set_paused(&mut ctx);
    common::close(&mut ctx, &sub).unwrap();
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}

#[test]
fn sponsored_rent_payer_is_recorded_and_refunded_on_close() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let sponsor = common::funded_keypair(&mut ctx);
    let member = sub.member.insecure_clone();
    let ixs = [
        common::rail_subscribe_ix(&ctx, &sub),
        common::ix::subscribe_hook_ix(
            sub.member.pubkey(),
            sponsor.pubkey(),
            sub.plan,
            sub.merchant.pubkey(),
            None,
        ),
        common::ix::collect_ix(common::collect_accounts(&sub, None)),
    ];
    common::send_many(&mut ctx, &ixs, &[&sponsor, &member]).unwrap();
    assert_eq!(
        common::fetch_membership(&ctx, &sub).rent_payer,
        sponsor.pubkey()
    );
    common::cancel_subscription_now(&mut ctx, &sub).unwrap();
    common::warp(&mut ctx, PERIOD + DAY);
    let wrong = common::close_with(&mut ctx, common::close_accounts(&sub, sub.member.pubkey()));
    assert!(common::anchor_err_is(&wrong, 2012));
    let membership = common::ix::membership_pda(&sub.plan, &sub.member.pubkey());
    let rent = ctx.svm.get_account(&membership).unwrap().lamports;
    let before = ctx.svm.get_balance(&sponsor.pubkey()).unwrap();
    common::close(&mut ctx, &sub).unwrap();
    assert_eq!(
        ctx.svm.get_balance(&sponsor.pubkey()).unwrap(),
        before + rent
    );
    assert!(ctx.svm.get_account(&membership).is_none());
}
