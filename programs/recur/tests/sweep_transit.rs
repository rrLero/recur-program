mod common;

use common::USDC;
use litesvm_token::MintTo;
use recur::RecurError;

fn stray(ctx: &mut common::Ctx, sub: &common::Sub, amount: u64) {
    let admin = ctx.admin.insecure_clone();
    MintTo::new(&mut ctx.svm, &admin, &sub.mint, &sub.transit_ata, amount)
        .send()
        .unwrap();
}

#[test]
fn sweep_transit_moves_stray_tokens_to_treasury() {
    let (mut ctx, sub) = common::configured_sub(30 * USDC, 720);
    stray(&mut ctx, &sub, 5 * USDC);
    let admin = ctx.admin.insecure_clone();
    common::sweep_transit(&mut ctx, &admin, sub.transit_ata, sub.treasury_ata).unwrap();
    assert_eq!(common::balance(&ctx, sub.transit_ata), 0);
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 5 * USDC);
}

#[test]
fn sweep_transit_rejects_non_admin() {
    let (mut ctx, sub) = common::configured_sub(30 * USDC, 720);
    stray(&mut ctx, &sub, 5 * USDC);
    let stranger = common::funded_keypair(&mut ctx);
    let result = common::sweep_transit(&mut ctx, &stranger, sub.transit_ata, sub.treasury_ata);
    assert!(common::err_is(&result, RecurError::Unauthorized));
    assert_eq!(common::balance(&ctx, sub.transit_ata), 5 * USDC);
}

#[test]
fn sweep_transit_rejects_foreign_treasury() {
    let (mut ctx, sub) = common::configured_sub(30 * USDC, 720);
    stray(&mut ctx, &sub, 5 * USDC);
    let admin = ctx.admin.insecure_clone();
    let result = common::sweep_transit(&mut ctx, &admin, sub.transit_ata, sub.merchant_ata);
    assert!(common::err_is(&result, RecurError::Unauthorized));
    assert_eq!(common::balance(&ctx, sub.transit_ata), 5 * USDC);
}

#[test]
fn sweep_transit_works_while_paused() {
    let (mut ctx, sub) = common::configured_sub(30 * USDC, 720);
    stray(&mut ctx, &sub, 5 * USDC);
    common::set_paused(&mut ctx);
    let admin = ctx.admin.insecure_clone();
    common::sweep_transit(&mut ctx, &admin, sub.transit_ata, sub.treasury_ata).unwrap();
    assert_eq!(common::balance(&ctx, sub.treasury_ata), 5 * USDC);
}
