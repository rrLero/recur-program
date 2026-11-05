mod common;

use common::USDC;
use recur::{fee_for, Membership, MembershipSource, RecurError, MEMBERSHIP_VERSION};
use solana_pubkey::Pubkey;
use solana_signer::Signer;

const PRICE: u64 = 30 * USDC;
const PERIOD: i64 = 720 * 3600;
const DAY: i64 = 86_400;
const RAIL_UNAUTHORIZED: u32 = 0x82;
const RAIL_SUBSCRIPTION_PLAN_MISMATCH: u32 = 0x1f9;
const RAIL_RECEIVER_NOT_A_DESTINATION: u32 = 0x1fa;

fn assert_untouched(ctx: &common::Ctx, sub: &common::Sub, paid_until: Option<i64>) {
    assert_eq!(common::balance(ctx, sub.transit_ata), 0);
    assert_eq!(common::paid_until(ctx, sub), paid_until);
}

#[test]
fn collect_splits_merchant_treasury_referrer() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, Some(sub.referrer)).unwrap();
    common::warp(&mut ctx, PERIOD);
    common::collect(&mut ctx, &sub, Some(sub.referrer_ata)).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 2 * 600_000);
    assert_eq!(common::balance(&ctx, sub.referrer_ata), 2 * 2_940_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 2 * 26_460_000);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
    assert_eq!(common::balance(&ctx, sub.member_ata), 940 * USDC);
}

#[test]
fn collect_without_referrer_pays_merchant_the_rest() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    common::warp(&mut ctx, PERIOD);
    common::collect(&mut ctx, &sub, None).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 2 * 600_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 2 * 29_400_000);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
}

#[test]
fn collect_twice_in_one_period_fails() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let paid_until = common::paid_until(&ctx, &sub);
    assert!(common::collect(&mut ctx, &sub, None).is_err());
    assert_untouched(&ctx, &sub, paid_until);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 29_400_000);
    assert_eq!(common::balance(&ctx, sub.member_ata), 970 * USDC);
}

#[test]
fn collect_with_insufficient_funds_leaves_transit_empty() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let paid_until = common::paid_until(&ctx, &sub);
    common::warp(&mut ctx, PERIOD);
    common::drain_member(&mut ctx, &sub);
    assert!(common::collect(&mut ctx, &sub, None).is_err());
    assert_untouched(&ctx, &sub, paid_until);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 29_400_000);
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
    common::rail_subscribe(&mut ctx, &sub).unwrap();
    let now = common::now(&ctx);
    common::write_membership(
        &mut ctx,
        &Membership {
            version: MEMBERSHIP_VERSION,
            bump: 0,
            member: sub.member.pubkey(),
            plan: sub.plan,
            merchant: sub.merchant.pubkey(),
            paid_until: 0,
            source: MembershipSource::Paid,
            referrer: None,
            referral_bps: 0,
            rent_payer: sub.member.pubkey(),
            created_at: now,
            reserved: [0; 64],
        },
    );
    let result = common::collect(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::NotRecurPuller));
    assert_untouched(&ctx, &sub, Some(0));
}

#[test]
fn collect_fails_when_paused() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let paid_until = common::paid_until(&ctx, &sub);
    common::warp(&mut ctx, PERIOD);
    common::set_paused(&mut ctx);
    let result = common::collect(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::Paused));
    assert_untouched(&ctx, &sub, paid_until);
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
fn collect_advances_paid_until_by_one_period() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let start = common::now(&ctx);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    common::warp(&mut ctx, PERIOD + DAY);
    common::collect(&mut ctx, &sub, None).unwrap();
    assert_eq!(common::paid_until(&ctx, &sub), Some(start + 2 * PERIOD));
}

#[test]
fn collect_after_long_lapse_restores_access() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let start = common::now(&ctx);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    common::drain_member(&mut ctx, &sub);
    common::warp(&mut ctx, PERIOD + PERIOD * 3 / 2);
    assert!(common::collect(&mut ctx, &sub, None).is_err());
    common::fund_member(&mut ctx, &sub, PRICE);
    ctx.svm.expire_blockhash();
    common::collect(&mut ctx, &sub, None).unwrap();
    let paid_until = common::paid_until(&ctx, &sub).unwrap();
    assert!(paid_until > common::now(&ctx));
    assert_eq!(paid_until, start + 3 * PERIOD);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
}

#[test]
fn collect_requires_the_recorded_referrer_account() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, Some(sub.referrer)).unwrap();
    let paid_until = common::paid_until(&ctx, &sub);
    common::warp(&mut ctx, PERIOD);
    let missing = common::collect(&mut ctx, &sub, None);
    assert!(common::err_is(&missing, RecurError::ReferrerMismatch));
    let stranger_ata = common::token_account(&mut ctx, &sub, &Pubkey::new_unique());
    let wrong = common::collect(&mut ctx, &sub, Some(stranger_ata));
    assert!(common::err_is(&wrong, RecurError::ReferrerMismatch));
    assert_untouched(&ctx, &sub, paid_until);
    assert_eq!(common::balance(&ctx, stranger_ata), 0);
    assert_eq!(common::balance(&ctx, sub.referrer_ata), 2_940_000);
}

#[test]
fn collect_ignores_caller_choice_of_referrer() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let paid_until = common::paid_until(&ctx, &sub);
    common::warp(&mut ctx, PERIOD);
    let result = common::collect(&mut ctx, &sub, Some(sub.referrer_ata));
    assert!(common::err_is(&result, RecurError::ReferrerMismatch));
    assert_untouched(&ctx, &sub, paid_until);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 29_400_000);
    assert_eq!(common::balance(&ctx, sub.referrer_ata), 0);
}

#[test]
fn collect_fee_free_window_pays_merchant_everything() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let admin = ctx.admin.insecure_clone();
    let until = common::now(&ctx) + 365 * DAY;
    common::set_merchant_terms(&mut ctx, &admin, sub.merchant.pubkey(), None, until).unwrap();
    common::subscribe(&mut ctx, &sub, None).unwrap();
    assert_eq!(common::balance(&ctx, sub.merchant_ata), PRICE);
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 0);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
}

#[test]
fn collect_uses_merchant_fee_override() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    let admin = ctx.admin.insecure_clone();
    common::set_merchant_terms(&mut ctx, &admin, sub.merchant.pubkey(), Some(100), 0).unwrap();
    common::subscribe(&mut ctx, &sub, None).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 300_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 29_700_000);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
}

#[test]
fn collect_with_revoked_delegate_fails() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let paid_until = common::paid_until(&ctx, &sub);
    common::revoke_delegate(&mut ctx, &sub).unwrap();
    common::warp(&mut ctx, PERIOD);
    assert!(common::collect(&mut ctx, &sub, None).is_err());
    assert_untouched(&ctx, &sub, paid_until);
    assert_eq!(common::balance(&ctx, sub.member_ata), 970 * USDC);
}

#[test]
fn collect_with_closed_member_account_fails() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let paid_until = common::paid_until(&ctx, &sub);
    common::close_member_ata(&mut ctx, &sub);
    common::warp(&mut ctx, PERIOD);
    assert!(common::collect(&mut ctx, &sub, None).is_err());
    assert_untouched(&ctx, &sub, paid_until);
}

#[test]
fn collect_without_membership_fails() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::rail_subscribe(&mut ctx, &sub).unwrap();
    assert!(common::collect(&mut ctx, &sub, None).is_err());
    assert_untouched(&ctx, &sub, None);
    assert_eq!(common::balance(&ctx, sub.member_ata), 1_000 * USDC);
}

#[test]
fn collect_refuses_while_comp_active() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    common::warp(&mut ctx, PERIOD);
    let comp_until = common::now(&ctx) + 60 * DAY;
    common::grant_comp(&mut ctx, &sub, comp_until).unwrap();
    let result = common::collect(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::PrepaidAccessActive));
    assert_untouched(&ctx, &sub, Some(comp_until));
    assert_eq!(
        common::fetch_membership(&ctx, &sub).source,
        MembershipSource::Comp
    );
    assert_eq!(common::balance(&ctx, sub.member_ata), 970 * USDC);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 29_400_000);
}

#[test]
fn collect_refuses_while_upfront_active() {
    let (mut ctx, mut sub) = common::configured_upfront(PRICE, 720);
    common::pay_upfront(&mut ctx, &sub, None).unwrap();
    let paid_until = common::paid_until(&ctx, &sub);
    common::set_plan(&mut ctx, &sub, |plan| {
        plan.data.pullers[0] = common::puller_pda()
    });
    sub.subscription = common::rail_subscription_pda(&sub.plan, &sub.member.pubkey());
    common::rail_subscribe(&mut ctx, &sub).unwrap();
    let merchant_before = common::balance(&ctx, sub.merchant_ata);
    let result = common::collect(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::PrepaidAccessActive));
    assert_untouched(&ctx, &sub, paid_until);
    assert_eq!(
        common::fetch_membership(&ctx, &sub).source,
        MembershipSource::Upfront
    );
    assert_eq!(common::balance(&ctx, sub.member_ata), 970 * USDC);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), merchant_before);
}

#[test]
fn collect_rejects_another_members_subscription() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let other = common::new_member(&mut ctx, &sub, true);
    common::subscribe(&mut ctx, &other, None).unwrap();
    let paid_until = common::paid_until(&ctx, &sub);
    common::warp(&mut ctx, PERIOD);
    let mut accounts = common::collect_accounts(&sub, None);
    accounts.subscription = other.subscription;
    let payer = common::funded_keypair(&mut ctx);
    let result = common::send(&mut ctx, common::ix::collect_ix(accounts), &[&payer]);
    assert!(common::anchor_err_is(&result, RAIL_UNAUTHORIZED));
    assert_untouched(&ctx, &sub, paid_until);
    assert_eq!(common::balance(&ctx, sub.member_ata), 970 * USDC);
    assert_eq!(common::balance(&ctx, other.member_ata), 970 * USDC);
}

#[test]
fn collect_rejects_another_plans_subscription() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let mut other = common::new_member(&mut ctx, &sub, false);
    other.member = sub.member.insecure_clone();
    other.member_ata = sub.member_ata;
    other.subscription_authority = sub.subscription_authority;
    other.plan = common::second_plan(&mut ctx, &sub);
    other.subscription = common::rail_subscription_pda(&other.plan, &sub.member.pubkey());
    common::rail_subscribe(&mut ctx, &other).unwrap();
    let paid_until = common::paid_until(&ctx, &sub);
    common::warp(&mut ctx, PERIOD);
    let mut accounts = common::collect_accounts(&sub, None);
    accounts.subscription = other.subscription;
    let payer = common::funded_keypair(&mut ctx);
    let result = common::send(&mut ctx, common::ix::collect_ix(accounts), &[&payer]);
    assert!(common::anchor_err_is(&result, RAIL_SUBSCRIPTION_PLAN_MISMATCH));
    assert_untouched(&ctx, &sub, paid_until);
    assert_eq!(common::balance(&ctx, sub.member_ata), 970 * USDC);
}

#[test]
fn collect_rejects_wrong_mint() {
    let (mut ctx, sub) = common::configured_sub(PRICE, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    let paid_until = common::paid_until(&ctx, &sub);
    common::warp(&mut ctx, PERIOD);
    let mint = common::new_mint(&mut ctx);
    let treasury = common::fetch_config(&ctx).treasury;
    let mut accounts = common::collect_accounts(&sub, None);
    accounts.mint = mint;
    accounts.transit_ata = common::ata(&mut ctx, &mint, &common::transit_pda());
    accounts.member_ata = common::ata(&mut ctx, &mint, &sub.member.pubkey());
    accounts.merchant_ata = common::ata(&mut ctx, &mint, &sub.merchant.pubkey());
    accounts.treasury_ata = common::ata(&mut ctx, &mint, &treasury);
    let payer = common::funded_keypair(&mut ctx);
    let result = common::send(&mut ctx, common::ix::collect_ix(accounts), &[&payer]);
    assert!(common::anchor_err_is(&result, 2502));
    assert_untouched(&ctx, &sub, paid_until);
    assert_eq!(common::balance(&ctx, sub.member_ata), 970 * USDC);
}

#[test]
fn agent_plan_activation_lands_in_transit() {
    let (mut ctx, sub, agent) = common::configured_agent_sub(PRICE, 720);
    let ixs = common::agent_activation_ixs(&ctx, &sub, &agent);
    common::send_many(&mut ctx, &ixs, &[&sub.member, &agent]).unwrap();
    assert_eq!(common::balance(&ctx, sub.transit_ata), PRICE);
    assert_eq!(common::balance(&ctx, sub.member_ata), 9 * PRICE);
}

#[test]
fn activation_to_a_non_destination_fails() {
    let (mut ctx, sub, agent) = common::configured_agent_sub(PRICE, 720);
    let mut ixs = common::agent_activation_ixs(&ctx, &sub, &agent);
    let transfer = ixs.last_mut().unwrap();
    for meta in transfer.accounts.iter_mut() {
        if meta.pubkey == sub.transit_ata {
            meta.pubkey = sub.merchant_ata;
        }
    }
    let result = common::send_many(&mut ctx, &ixs, &[&sub.member, &agent]);
    assert!(common::anchor_err_is(&result, RAIL_RECEIVER_NOT_A_DESTINATION));
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 0);
    assert_eq!(common::balance(&ctx, sub.member_ata), 10 * PRICE);
}
