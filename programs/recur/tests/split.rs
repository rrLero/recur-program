mod common;

use common::{Ctx, Sub, USDC};
use recur::RecurError;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;

const DAY: i64 = 86_400;
const HELD: u64 = 5 * USDC;
const ANCHOR_CONSTRAINT_RAW: u32 = 2003;
const ANCHOR_CONSTRAINT_SEEDS: u32 = 2006;

fn update_fees(ctx: &mut Ctx, fee_bps: u16, min_fee: u64, agent_puller: Option<Pubkey>) {
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
        agent_puller.unwrap_or(cfg.agent_puller),
    )
    .unwrap();
}

fn funded_transit() -> (Ctx, Sub, Keypair) {
    let (mut ctx, sub, agent) = common::configured_agent_sub(USDC, 720);
    update_fees(&mut ctx, 200, 500_000, None);
    common::fund_transit(&mut ctx, &sub, HELD);
    (ctx, sub, agent)
}

fn assert_nothing_paid(ctx: &Ctx, sub: &Sub) {
    assert_eq!(common::balance(ctx, sub.transit_ata), HELD);
    assert_eq!(common::balance(ctx, sub.treasury_ata), 0);
    assert_eq!(common::balance(ctx, sub.merchant_ata), 0);
}

#[test]
fn splits_bps_only() {
    let (mut ctx, sub, agent) = funded_transit();
    common::split(&mut ctx, &sub, &agent, USDC).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 20_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 980_000);
    assert_eq!(common::balance(&ctx, sub.transit_ata), HELD - USDC);
}

#[test]
fn splits_with_the_merchant_fee_override() {
    let (mut ctx, sub, agent) = funded_transit();
    let admin = ctx.admin.insecure_clone();
    common::set_merchant_terms(&mut ctx, &admin, sub.merchant.pubkey(), Some(100), 0).unwrap();
    common::split(&mut ctx, &sub, &agent, USDC).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 10_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), 990_000);
}

#[test]
fn fee_free_window_pays_merchant_all() {
    let (mut ctx, sub, agent) = funded_transit();
    let admin = ctx.admin.insecure_clone();
    let until = common::now(&ctx) + 365 * DAY;
    common::set_merchant_terms(&mut ctx, &admin, sub.merchant.pubkey(), None, until).unwrap();
    common::split(&mut ctx, &sub, &agent, USDC).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 0);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), USDC);
    assert_eq!(common::balance(&ctx, sub.transit_ata), HELD - USDC);
}

#[test]
fn splits_the_whole_transit_balance() {
    let (mut ctx, sub, agent) = funded_transit();
    common::split(&mut ctx, &sub, &agent, HELD).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 100_000);
    assert_eq!(common::balance(&ctx, sub.merchant_ata), HELD - 100_000);
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
}

#[test]
fn refuses_a_zero_amount() {
    let (mut ctx, sub, agent) = funded_transit();
    let result = common::split(&mut ctx, &sub, &agent, 0);
    assert!(common::err_is(&result, RecurError::InsufficientTransit));
    assert_nothing_paid(&ctx, &sub);
}

#[test]
fn refuses_more_than_the_transit_holds() {
    let (mut ctx, sub, agent) = funded_transit();
    let result = common::split(&mut ctx, &sub, &agent, HELD + 1);
    assert!(common::err_is(&result, RecurError::InsufficientTransit));
    assert_nothing_paid(&ctx, &sub);
}

#[test]
fn refuses_while_paused() {
    let (mut ctx, sub, agent) = funded_transit();
    common::set_paused(&mut ctx);
    let result = common::split(&mut ctx, &sub, &agent, USDC);
    assert!(common::err_is(&result, RecurError::Paused));
    assert_nothing_paid(&ctx, &sub);
}

#[test]
fn refuses_when_the_agent_puller_is_not_set() {
    let (mut ctx, sub, agent) = funded_transit();
    update_fees(&mut ctx, 200, 500_000, Some(Pubkey::default()));
    let result = common::split(&mut ctx, &sub, &agent, USDC);
    assert!(common::err_is(&result, RecurError::AgentPullerNotSet));
    assert_nothing_paid(&ctx, &sub);
}

#[test]
fn refuses_a_signer_other_than_the_agent_puller() {
    let (mut ctx, sub, _) = funded_transit();
    let merchant = sub.merchant.insecure_clone();
    let accounts = common::split_accounts(&sub, merchant.pubkey());
    let result = common::split_with(&mut ctx, &merchant, accounts, USDC);
    assert!(common::err_is(&result, RecurError::Unauthorized));
    assert_nothing_paid(&ctx, &sub);
}

#[test]
fn refuses_a_merchant_ata_the_merchant_does_not_own() {
    let (mut ctx, sub, agent) = funded_transit();
    let mut accounts = common::split_accounts(&sub, agent.pubkey());
    accounts.merchant_ata = sub.referrer_ata;
    let result = common::split_with(&mut ctx, &agent, accounts, USDC);
    assert!(common::anchor_err_is(&result, ANCHOR_CONSTRAINT_RAW));
    assert_nothing_paid(&ctx, &sub);
    assert_eq!(common::balance(&ctx, sub.referrer_ata), 0);
}

#[test]
fn refuses_a_treasury_ata_the_treasury_does_not_own() {
    let (mut ctx, sub, agent) = funded_transit();
    let mut accounts = common::split_accounts(&sub, agent.pubkey());
    accounts.treasury_ata = sub.referrer_ata;
    let result = common::split_with(&mut ctx, &agent, accounts, USDC);
    assert!(common::anchor_err_is(&result, ANCHOR_CONSTRAINT_RAW));
    assert_nothing_paid(&ctx, &sub);
    assert_eq!(common::balance(&ctx, sub.referrer_ata), 0);
}

#[test]
fn refuses_another_merchants_transit() {
    let (mut ctx, sub, agent) = funded_transit();
    let other = common::funded_keypair(&mut ctx);
    common::register_merchant_as(&mut ctx, &other, 0).unwrap();
    let other_ata = common::token_account(&mut ctx, &sub, &other.pubkey());
    let mut accounts = common::split_accounts(&sub, agent.pubkey());
    accounts.merchant = common::merchant_pda(&other.pubkey());
    accounts.merchant_ata = other_ata;
    let result = common::split_with(&mut ctx, &agent, accounts, USDC);
    assert!(common::anchor_err_is(&result, ANCHOR_CONSTRAINT_SEEDS));
    assert_nothing_paid(&ctx, &sub);
    assert_eq!(common::balance(&ctx, other_ata), 0);
}
