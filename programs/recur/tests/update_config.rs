mod common;

use recur::RecurError;
use solana_signer::Signer;

fn initialised() -> (common::Ctx, solana_pubkey::Pubkey) {
    let mut ctx = common::setup();
    let treasury = common::new_address();
    common::init_config(&mut ctx, 200, 100_000, treasury).unwrap();
    (ctx, treasury)
}

#[test]
fn update_config_replaces_fields() {
    let (mut ctx, _) = initialised();
    let admin = ctx.admin.insecure_clone();
    let new_treasury = common::new_address();
    common::update_config(
        &mut ctx,
        &admin,
        admin.pubkey(),
        500,
        50_000,
        new_treasury,
        true,
    )
    .unwrap();
    let cfg = common::fetch_config(&ctx);
    assert_eq!(
        (
            cfg.admin,
            cfg.fee_bps,
            cfg.min_fee,
            cfg.treasury,
            cfg.paused
        ),
        (admin.pubkey(), 500, 50_000, new_treasury, true)
    );
}

#[test]
fn update_config_rejects_non_admin() {
    let (mut ctx, treasury) = initialised();
    let stranger = common::funded_keypair(&mut ctx);
    let result = common::update_config(
        &mut ctx,
        &stranger,
        stranger.pubkey(),
        0,
        0,
        treasury,
        false,
    );
    assert!(common::err_is(&result, RecurError::Unauthorized));
    assert_eq!(common::fetch_config(&ctx).admin, ctx.admin.pubkey());
}

#[test]
fn update_config_rejects_fee_over_100_percent() {
    let (mut ctx, treasury) = initialised();
    let admin = ctx.admin.insecure_clone();
    let result =
        common::update_config(&mut ctx, &admin, admin.pubkey(), 10_001, 0, treasury, false);
    assert!(common::err_is(&result, RecurError::FeeTooHigh));
}

#[test]
fn update_config_can_hand_admin_over() {
    let (mut ctx, treasury) = initialised();
    let old = ctx.admin.insecure_clone();
    let new = common::funded_keypair(&mut ctx);
    common::update_config(&mut ctx, &old, new.pubkey(), 200, 100_000, treasury, false).unwrap();
    ctx.svm.expire_blockhash();
    common::update_config(&mut ctx, &new, new.pubkey(), 300, 100_000, treasury, false).unwrap();
    ctx.svm.expire_blockhash();
    let result = common::update_config(&mut ctx, &old, old.pubkey(), 400, 100_000, treasury, false);
    assert!(common::err_is(&result, RecurError::Unauthorized));
    assert_eq!(common::fetch_config(&ctx).fee_bps, 300);
}

#[test]
fn update_config_pauses_collect() {
    let (mut ctx, sub) = common::configured_sub(30 * common::USDC, 720);
    common::set_paused(&mut ctx);
    assert!(common::fetch_config(&ctx).paused);
    let result = common::collect(&mut ctx, &sub, 30 * common::USDC, None);
    assert!(common::err_is(&result, RecurError::Paused));
}

#[test]
fn warp_moves_the_clock_forward() {
    let mut ctx = common::setup();
    let before = common::now(&ctx);
    common::warp(&mut ctx, 3_600);
    assert_eq!(common::now(&ctx), before + 3_600);
}
