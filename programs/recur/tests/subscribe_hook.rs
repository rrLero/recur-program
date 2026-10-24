mod common;

use common::USDC;
use recur::{MembershipSource, RecurError, MEMBERSHIP_VERSION};
use solana_pubkey::Pubkey;
use solana_signer::Signer;

const PRICE: u64 = 30 * USDC;
const PERIOD: i64 = 720 * 3600;
const DAY: i64 = 86_400;

#[test]
fn subscribe_bundle_opens_membership_and_collects_first_period() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let start = common::now(&ctx);
    common::subscribe(&mut ctx, &sub, Some(sub.referrer)).unwrap();
    let membership = common::fetch_membership(&ctx, &sub);
    assert_eq!(membership.paid_until, start + PERIOD);
    assert_eq!(membership.source, MembershipSource::Paid);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 26_460_000);
    assert_eq!(common::balance(&ctx, sub.referrer_ata), 2_940_000);
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 600_000);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
}

#[test]
fn subscribe_hook_records_merchant_referral_rate() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, Some(sub.referrer)).unwrap();
    let membership = common::fetch_membership(&ctx, &sub);
    assert_eq!(membership.version, MEMBERSHIP_VERSION);
    assert_eq!(membership.member, sub.member.pubkey());
    assert_eq!(membership.plan, sub.plan);
    assert_eq!(membership.merchant, sub.merchant.pubkey());
    assert_eq!(membership.rent_payer, sub.member.pubkey());
    assert_eq!(membership.referrer, Some(sub.referrer));
    assert_eq!(membership.referral_bps, 1_000);
}

#[test]
fn subscribe_hook_without_referrer_records_no_share() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let membership = common::fetch_membership(&ctx, &sub);
    assert_eq!(membership.referrer, None);
    assert_eq!(membership.referral_bps, 0);
}

#[test]
fn subscribe_hook_rejects_self_referral() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let member = sub.member.pubkey();
    let result = common::subscribe(&mut ctx, &sub, Some(member));
    assert!(common::err_at(&result, 1, RecurError::SelfReferral));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
    assert_eq!(common::balance(&ctx, sub.member_ata), 1_000 * USDC);
}

#[test]
fn subscribe_hook_keeps_referrer_while_active() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, Some(sub.referrer)).unwrap();
    let before = common::fetch_membership(&ctx, &sub);
    let result = common::subscribe_hook(&mut ctx, &sub, Some(Pubkey::new_unique()));
    assert!(common::err_is(&result, RecurError::AlreadyMember));
    let after = common::fetch_membership(&ctx, &sub);
    assert_eq!(after.referrer, Some(sub.referrer));
    assert_eq!(after.paid_until, before.paid_until);
}

#[test]
fn subscribe_hook_reopens_expired_membership_with_new_referrer() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let start = common::now(&ctx);
    common::subscribe(&mut ctx, &sub, Some(sub.referrer)).unwrap();
    let created_at = common::fetch_membership(&ctx, &sub).created_at;
    let new_referrer = Pubkey::new_unique();
    let new_referrer_ata = common::token_account(&mut ctx, &sub, &new_referrer);
    common::warp(&mut ctx, PERIOD + 1);
    common::renew(&mut ctx, &sub, Some(new_referrer)).unwrap();
    let membership = common::fetch_membership(&ctx, &sub);
    assert_eq!(membership.referrer, Some(new_referrer));
    assert_eq!(membership.created_at, created_at);
    assert_eq!(membership.paid_until, start + 2 * PERIOD);
    assert_eq!(common::balance(&ctx, new_referrer_ata), 2_940_000);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
}

#[test]
fn subscribe_hook_on_comp_membership_never_shortens_access() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    common::grant_comp(&mut ctx, &sub, now + 60 * DAY).unwrap();
    common::subscribe(&mut ctx, &sub, Some(sub.referrer)).unwrap();
    let membership = common::fetch_membership(&ctx, &sub);
    assert_eq!(membership.paid_until, now + 60 * DAY);
    assert_eq!(membership.source, MembershipSource::Paid);
    assert_eq!(membership.referrer, Some(sub.referrer));
    assert_eq!(membership.rent_payer, sub.merchant.pubkey());
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
}

#[test]
fn subscribe_hook_rejects_sunset_plan() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::set_plan(&mut ctx, &sub, |plan| plan.status = 0);
    let result = common::subscribe_hook(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::PlanNotActive));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}

#[test]
fn subscribe_hook_rejects_ended_plan() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    common::set_plan(&mut ctx, &sub, |plan| plan.data.end_ts = now - 1);
    let result = common::subscribe_hook(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::PlanNotActive));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}

#[test]
fn subscribe_hook_rejects_plan_without_recur_puller() {
    let pullers = [
        Pubkey::new_unique(),
        Pubkey::default(),
        Pubkey::default(),
        Pubkey::default(),
    ];
    let (mut ctx, sub) = common::configured_sub_with_pullers(PRICE, 720, pullers);
    let result = common::subscribe_hook(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::NotRecurPuller));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}

#[test]
fn subscribe_hook_fails_when_paused() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::set_paused(&mut ctx);
    let result = common::subscribe_hook(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::Paused));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}

#[test]
fn subscribe_hook_alone_keeps_comp_source() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    common::grant_comp(&mut ctx, &sub, now + 60 * DAY).unwrap();
    common::subscribe_hook(&mut ctx, &sub, Some(sub.referrer)).unwrap();
    let membership = common::fetch_membership(&ctx, &sub);
    assert_eq!(membership.source, MembershipSource::Comp);
    assert_eq!(membership.paid_until, now + 60 * DAY);
}

#[test]
fn subscribe_hook_rejects_recur_pda_as_referrer() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let result = common::subscribe_hook(&mut ctx, &sub, Some(common::transit_pda()));
    assert!(common::err_is(&result, RecurError::SelfReferral));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}
