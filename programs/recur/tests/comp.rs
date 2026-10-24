mod common;

use common::USDC;
use recur::{MembershipSource, RecurError};
use solana_pubkey::Pubkey;
use solana_signer::Signer;

const PRICE: u64 = 30 * USDC;
const DAY: i64 = 86_400;

#[test]
fn grant_comp_creates_comp_membership() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    common::grant_comp(&mut ctx, &sub, now + 30 * DAY).unwrap();
    let membership = common::fetch_membership(&ctx, &sub);
    assert_eq!(membership.paid_until, now + 30 * DAY);
    assert_eq!(membership.source, MembershipSource::Comp);
    assert_eq!(membership.member, sub.member.pubkey());
    assert_eq!(membership.plan, sub.plan);
    assert_eq!(membership.merchant, sub.merchant.pubkey());
    assert_eq!(membership.rent_payer, sub.merchant.pubkey());
    assert_eq!(membership.created_at, now);
    assert_eq!(membership.referrer, None);
    assert_eq!(membership.referral_bps, 0);
    assert_eq!(common::balance(&ctx, sub.member_ata), 1_000 * USDC);
}

#[test]
fn grant_comp_batch_of_three_in_one_tx() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    let members: Vec<Pubkey> = (0..3).map(|_| Pubkey::new_unique()).collect();
    let ixs: Vec<_> = members
        .iter()
        .map(|member| common::grant_comp_ix(&sub, *member, now + 10 * DAY))
        .collect();
    let merchant = sub.merchant.insecure_clone();
    common::send_many(&mut ctx, &ixs, &[&merchant]).unwrap();
    for member in members {
        let account = ctx
            .svm
            .get_account(&common::ix::membership_pda(&sub.plan, &member))
            .expect("membership exists");
        assert_eq!(account.owner, recur::ID);
    }
}

#[test]
fn grant_comp_rejects_non_plan_owner() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    let stranger = common::funded_keypair(&mut ctx);
    let ix = common::ix::grant_comp_ix(
        stranger.pubkey(),
        stranger.pubkey(),
        sub.plan,
        sub.member.pubkey(),
        now + DAY,
    );
    let result = common::send(&mut ctx, ix, &[&stranger]);
    assert!(common::err_is(&result, RecurError::Unauthorized));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}

#[test]
fn grant_comp_rejects_past_date() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    for until in [now - 1, now] {
        let result = common::grant_comp(&mut ctx, &sub, until);
        assert!(common::err_is(&result, RecurError::InvalidCompEnd));
        ctx.svm.expire_blockhash();
    }
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
}

#[test]
fn grant_comp_refuses_active_paid_member() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let before = common::fetch_membership(&ctx, &sub);
    let now = common::now(&ctx);
    let result = common::grant_comp(&mut ctx, &sub, now + 90 * DAY);
    assert!(common::err_is(&result, RecurError::AlreadyMember));
    let after = common::fetch_membership(&ctx, &sub);
    assert_eq!(after.paid_until, before.paid_until);
    assert_eq!(after.source, MembershipSource::Paid);
}

#[test]
fn revoke_comp_ends_access_now() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    common::grant_comp(&mut ctx, &sub, now + 30 * DAY).unwrap();
    common::warp(&mut ctx, DAY);
    common::revoke_comp(&mut ctx, &sub).unwrap();
    let membership = common::fetch_membership(&ctx, &sub);
    assert_eq!(membership.paid_until, now + DAY);
    assert_eq!(membership.source, MembershipSource::Comp);
}

#[test]
fn revoke_comp_rejects_paid_membership() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let before = common::paid_until(&ctx, &sub);
    let result = common::revoke_comp(&mut ctx, &sub);
    assert!(common::err_is(&result, RecurError::NotComp));
    assert_eq!(common::paid_until(&ctx, &sub), before);
}

#[test]
fn revoke_comp_rejects_non_plan_owner() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let now = common::now(&ctx);
    common::grant_comp(&mut ctx, &sub, now + 30 * DAY).unwrap();
    let stranger = common::funded_keypair(&mut ctx);
    let result = common::revoke_comp_as(&mut ctx, &sub, &stranger);
    assert!(common::err_is(&result, RecurError::Unauthorized));
    assert_eq!(common::paid_until(&ctx, &sub), Some(now + 30 * DAY));
}

#[test]
fn comp_converts_to_paid_at_comp_end() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let comp_until = common::now(&ctx) + 60 * DAY;
    common::grant_comp(&mut ctx, &sub, comp_until).unwrap();
    common::warp(&mut ctx, 60 * DAY);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let membership = common::fetch_membership(&ctx, &sub);
    assert_eq!(membership.source, MembershipSource::Paid);
    assert_eq!(membership.paid_until, comp_until + 720 * 3600);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 29_400_000);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
}
