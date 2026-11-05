mod common;

use common::{Ctx, Sub, USDC};
use recur::{Membership, MembershipSource, RecurError, MEMBERSHIP_VERSION};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;

const PRICE: u64 = 10 * USDC;
const PERIOD: i64 = 720 * 3600;
const DAY: i64 = 86_400;

fn set_fees(ctx: &mut Ctx, fee_bps: u16, min_fee: u64) {
    let cfg = common::fetch_config(ctx);
    let admin = ctx.admin.insecure_clone();
    common::update_config(
        ctx,
        &admin,
        cfg.admin,
        fee_bps,
        min_fee,
        cfg.treasury,
        cfg.paused,
        cfg.agent_puller,
    )
    .unwrap();
}

fn set_agent_puller(ctx: &mut Ctx, agent_puller: Pubkey) {
    let cfg = common::fetch_config(ctx);
    let admin = ctx.admin.insecure_clone();
    common::update_config(
        ctx,
        &admin,
        cfg.admin,
        cfg.fee_bps,
        cfg.min_fee,
        cfg.treasury,
        cfg.paused,
        agent_puller,
    )
    .unwrap();
}

fn agent_sub(price: u64) -> (Ctx, Sub, Keypair) {
    let (mut ctx, sub, agent) = common::configured_agent_sub(price, 720);
    set_fees(&mut ctx, 200, 0);
    (ctx, sub, agent)
}

fn activated(price: u64) -> (Ctx, Sub, Keypair) {
    let (mut ctx, sub, agent) = agent_sub(price);
    let ixs = common::agent_activation_ixs(&ctx, &sub, &agent);
    common::send_many(&mut ctx, &ixs, &[&sub.member, &agent]).unwrap();
    (ctx, sub, agent)
}

fn period_start(ctx: &Ctx, sub: &Sub) -> i64 {
    let account = ctx.svm.get_account(&sub.subscription).unwrap();
    subscriptions::SubscriptionDelegation::from_bytes(&account.data)
        .unwrap()
        .current_period_start_ts
}

fn assert_nothing_paid(ctx: &Ctx, sub: &Sub, transit: u64) {
    assert_eq!(common::balance(ctx, sub.transit_ata), transit);
    assert_eq!(common::balance(ctx, sub.treasury_ata), 0);
    assert_eq!(common::balance(ctx, sub.merchant_ata), 0);
}

fn existing_membership(sub: &Sub, source: MembershipSource, paid_until: i64) -> Membership {
    Membership {
        version: MEMBERSHIP_VERSION,
        bump: 0,
        member: sub.member.pubkey(),
        plan: sub.plan,
        merchant: sub.merchant.pubkey(),
        paid_until,
        source,
        referrer: None,
        referral_bps: 0,
        rent_payer: sub.merchant.pubkey(),
        created_at: paid_until - DAY,
        reserved: [0; 64],
    }
}

#[test]
fn activates_and_splits_the_first_period() {
    let (mut ctx, sub, agent) = agent_sub(PRICE);
    let transit_before = common::balance(&ctx, sub.transit_ata);
    let ixs = common::agent_activation_ixs(&ctx, &sub, &agent);
    common::send_many(&mut ctx, &ixs, &[&sub.member, &agent]).unwrap();
    common::activate_agent(&mut ctx, &sub, &agent).unwrap();

    let membership = common::fetch_membership(&ctx, &sub);
    assert_eq!(membership.source, MembershipSource::Paid);
    assert_eq!(membership.member, sub.member.pubkey());
    assert_eq!(membership.plan, sub.plan);
    assert_eq!(membership.merchant, sub.merchant.pubkey());
    assert_eq!(membership.paid_until, period_start(&ctx, &sub) + PERIOD);
    assert_eq!(membership.referrer, None);
    assert_eq!(membership.referral_bps, 0);
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 200_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 9_800_000);
    assert_eq!(common::balance(&ctx, sub.transit_ata), transit_before);
    assert_eq!(common::balance(&ctx, sub.member_ata), 9 * PRICE);
}

#[test]
fn applies_min_fee_to_the_period() {
    let (mut ctx, sub, agent) = agent_sub(USDC);
    set_fees(&mut ctx, 200, 500_000);
    let ixs = common::agent_activation_ixs(&ctx, &sub, &agent);
    common::send_many(&mut ctx, &ixs, &[&sub.member, &agent]).unwrap();
    common::activate_agent(&mut ctx, &sub, &agent).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 500_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 500_000);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
}

#[test]
fn refuses_while_paused() {
    let (mut ctx, sub, agent) = activated(PRICE);
    common::set_paused(&mut ctx);
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::Paused));
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_when_the_agent_puller_is_not_set() {
    let (mut ctx, sub, agent) = activated(PRICE);
    set_agent_puller(&mut ctx, Pubkey::default());
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::AgentPullerNotSet));
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_a_signer_other_than_the_agent_puller() {
    let (mut ctx, sub, _agent) = activated(PRICE);
    let impostor = Keypair::new();
    let result = common::activate_agent(&mut ctx, &sub, &impostor);
    assert!(common::err_is(&result, RecurError::Unauthorized));
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_a_plan_with_an_extra_destination() {
    let (mut ctx, sub, agent) = activated(PRICE);
    let merchant = sub.merchant.pubkey();
    common::set_plan(&mut ctx, &sub, |plan| plan.data.destinations[1] = merchant);
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::NotAgentPlan));
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_a_plan_that_pays_the_merchant_directly() {
    let (mut ctx, sub, agent) = activated(PRICE);
    let merchant = sub.merchant.pubkey();
    common::set_plan(&mut ctx, &sub, |plan| plan.data.destinations[0] = merchant);
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::NotAgentPlan));
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_a_plan_without_the_agent_puller() {
    let (mut ctx, sub, agent) = activated(PRICE);
    common::set_plan(&mut ctx, &sub, |plan| {
        plan.data.pullers[0] = Pubkey::default()
    });
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::NotRecurPuller));
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_a_plan_without_the_recur_puller() {
    let (mut ctx, sub, agent) = activated(PRICE);
    common::set_plan(&mut ctx, &sub, |plan| {
        plan.data.pullers[1] = Pubkey::default()
    });
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::NotRecurPuller));
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_a_period_that_was_not_pulled() {
    let (mut ctx, sub, agent) = agent_sub(PRICE);
    let mut ixs = common::agent_activation_ixs(&ctx, &sub, &agent);
    ixs.pop();
    common::send_many(&mut ctx, &ixs, &[&sub.member]).unwrap();
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::PeriodNotPulled));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
    assert_nothing_paid(&ctx, &sub, 0);
}

#[test]
fn refuses_to_settle_the_same_period_twice() {
    let (mut ctx, sub, agent) = activated(PRICE);
    common::activate_agent(&mut ctx, &sub, &agent).unwrap();
    let paid_until = common::paid_until(&ctx, &sub);
    let admin = ctx.admin.insecure_clone();
    litesvm_token::MintTo::new(&mut ctx.svm, &admin, &sub.mint, &sub.transit_ata, PRICE)
        .send()
        .unwrap();
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::PeriodAlreadySettled));
    assert_eq!(common::paid_until(&ctx, &sub), paid_until);
    assert_eq!(common::balance(&ctx, sub.transit_ata), PRICE);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 9_800_000);
}

#[test]
fn refuses_while_a_comp_is_active() {
    let (mut ctx, sub, agent) = activated(PRICE);
    let until = common::now(&ctx) + DAY;
    common::write_membership(
        &mut ctx,
        &existing_membership(&sub, MembershipSource::Comp, until),
    );
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::AlreadyMember));
    assert_eq!(common::paid_until(&ctx, &sub), Some(until));
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_while_an_upfront_membership_is_active() {
    let (mut ctx, sub, agent) = activated(PRICE);
    let until = common::now(&ctx) + DAY;
    common::write_membership(
        &mut ctx,
        &existing_membership(&sub, MembershipSource::Upfront, until),
    );
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::AlreadyMember));
    assert_eq!(common::paid_until(&ctx, &sub), Some(until));
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn activates_over_an_expired_comp() {
    let (mut ctx, sub, agent) = activated(PRICE);
    let until = common::now(&ctx) - 1;
    common::write_membership(
        &mut ctx,
        &existing_membership(&sub, MembershipSource::Comp, until),
    );
    common::activate_agent(&mut ctx, &sub, &agent).unwrap();
    let membership = common::fetch_membership(&ctx, &sub);
    assert_eq!(membership.source, MembershipSource::Paid);
    assert_eq!(membership.paid_until, period_start(&ctx, &sub) + PERIOD);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
}

const ANCHOR_CONSTRAINT_RAW: u32 = 2003;
const ANCHOR_REQUIRE_KEYS_EQ_VIOLATED: u32 = 2502;

#[test]
fn rejects_settling_an_ended_period_after_close() {
    let (mut ctx, sub, agent) = activated(PRICE);
    common::activate_agent(&mut ctx, &sub, &agent).unwrap();
    common::cancel_subscription_now(&mut ctx, &sub).unwrap();
    common::warp(&mut ctx, PERIOD + DAY);
    common::close(&mut ctx, &sub).unwrap();
    let admin = ctx.admin.insecure_clone();
    litesvm_token::MintTo::new(&mut ctx.svm, &admin, &sub.mint, &sub.transit_ata, PRICE)
        .send()
        .unwrap();
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::PeriodEnded));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
    assert_eq!(common::balance(&ctx, sub.transit_ata), PRICE);
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 200_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 9_800_000);
}

#[test]
fn refuses_a_mint_other_than_the_plan_mint() {
    let (mut ctx, sub, agent) = activated(PRICE);
    let mint = common::new_mint(&mut ctx);
    let treasury = common::fetch_config(&ctx).treasury;
    let transit_ata = common::ata(
        &mut ctx,
        &mint,
        &common::merchant_transit_pda(&sub.merchant.pubkey()),
    );
    let merchant_ata = common::ata(&mut ctx, &mint, &sub.merchant.pubkey());
    let treasury_ata = common::ata(&mut ctx, &mint, &treasury);
    let result = common::activate_agent_with(&mut ctx, &sub, &agent, |accounts| {
        accounts.mint = mint;
        accounts.transit_ata = transit_ata;
        accounts.merchant_ata = merchant_ata;
        accounts.treasury_ata = treasury_ata;
    });
    assert!(common::anchor_err_is(&result, ANCHOR_REQUIRE_KEYS_EQ_VIOLATED));
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_a_merchant_that_does_not_own_the_plan() {
    let (mut ctx, sub, agent) = activated(PRICE);
    let other = common::funded_keypair(&mut ctx);
    common::register_merchant_as(&mut ctx, &other, 0).unwrap();
    let other_transit = common::merchant_transit_pda(&other.pubkey());
    let transit_ata = common::ata(&mut ctx, &sub.mint, &other_transit);
    let merchant_ata = common::ata(&mut ctx, &sub.mint, &other.pubkey());
    common::set_plan(&mut ctx, &sub, |plan| {
        plan.data.destinations[0] = other_transit
    });
    let result = common::activate_agent_with(&mut ctx, &sub, &agent, |accounts| {
        accounts.merchant = common::merchant_pda(&other.pubkey());
        accounts.transit = other_transit;
        accounts.transit_ata = transit_ata;
        accounts.merchant_ata = merchant_ata;
    });
    assert!(common::anchor_err_is(&result, ANCHOR_REQUIRE_KEYS_EQ_VIOLATED));
    assert_eq!(common::balance(&ctx, merchant_ata), 0);
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_a_subscription_delegated_by_someone_else() {
    let (mut ctx, sub, agent) = activated(PRICE);
    common::set_subscription(&mut ctx, &sub, |subscription| {
        subscription.header.delegator = Pubkey::new_unique()
    });
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::Unauthorized));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_a_subscription_to_another_plan() {
    let (mut ctx, sub, agent) = activated(PRICE);
    common::set_subscription(&mut ctx, &sub, |subscription| {
        subscription.header.delegatee = Pubkey::new_unique()
    });
    let result = common::activate_agent(&mut ctx, &sub, &agent);
    assert!(common::err_is(&result, RecurError::Unauthorized));
    assert!(common::try_fetch_membership(&ctx, &sub).is_none());
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_a_merchant_ata_the_merchant_does_not_own() {
    let (mut ctx, sub, agent) = activated(PRICE);
    let result = common::activate_agent_with(&mut ctx, &sub, &agent, |accounts| {
        accounts.merchant_ata = sub.referrer_ata
    });
    assert!(common::anchor_err_is(&result, ANCHOR_CONSTRAINT_RAW));
    assert_eq!(common::balance(&ctx, sub.referrer_ata), 0);
    assert_nothing_paid(&ctx, &sub, PRICE);
}

#[test]
fn refuses_a_treasury_ata_the_treasury_does_not_own() {
    let (mut ctx, sub, agent) = activated(PRICE);
    let result = common::activate_agent_with(&mut ctx, &sub, &agent, |accounts| {
        accounts.treasury_ata = sub.referrer_ata
    });
    assert!(common::anchor_err_is(&result, ANCHOR_CONSTRAINT_RAW));
    assert_eq!(common::balance(&ctx, sub.referrer_ata), 0);
    assert_nothing_paid(&ctx, &sub, PRICE);
}
