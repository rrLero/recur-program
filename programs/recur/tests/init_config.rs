mod common;

use recur::{RecurError, MAX_FEE_BPS, MAX_MIN_FEE};
use solana_signer::Signer;

#[test]
fn init_config_stores_fee_and_admin() {
    let mut ctx = common::setup();
    let treasury = common::new_address();
    common::init_config(&mut ctx, 200, 100_000, treasury).unwrap();
    let cfg = common::fetch_config(&ctx);
    assert_eq!(
        (cfg.version, cfg.fee_bps, cfg.min_fee, cfg.paused),
        (2, 200, 100_000, false)
    );
    assert_eq!(cfg.admin, ctx.admin.pubkey());
    assert_eq!(cfg.treasury, treasury);
}

#[test]
fn init_config_twice_fails() {
    let mut ctx = common::setup();
    let treasury = common::new_address();
    common::init_config(&mut ctx, 200, 100_000, treasury).unwrap();
    ctx.svm.expire_blockhash();
    assert!(common::init_config(&mut ctx, 200, 100_000, treasury).is_err());
}

#[test]
fn init_config_accepts_fees_at_the_caps() {
    let mut ctx = common::setup();
    common::init_config(&mut ctx, MAX_FEE_BPS, MAX_MIN_FEE, common::new_address()).unwrap();
    let cfg = common::fetch_config(&ctx);
    assert_eq!((cfg.fee_bps, cfg.min_fee), (MAX_FEE_BPS, MAX_MIN_FEE));
}

#[test]
fn init_config_rejects_fee_over_the_cap() {
    let mut ctx = common::setup();
    let result = common::init_config(&mut ctx, MAX_FEE_BPS + 1, 0, common::new_address());
    assert!(common::err_is(&result, RecurError::FeeTooHigh));
}

#[test]
fn init_config_rejects_min_fee_over_the_cap() {
    let mut ctx = common::setup();
    let result = common::init_config(&mut ctx, 200, MAX_MIN_FEE + 1, common::new_address());
    assert!(common::err_is(&result, RecurError::FeeTooHigh));
}

#[test]
fn init_config_rejects_non_upgrade_authority() {
    let mut ctx = common::setup();
    let stranger = common::funded_keypair(&mut ctx);
    let result = common::init_config_as(&mut ctx, &stranger, 200, 100_000, common::new_address());
    assert!(common::err_is(&result, RecurError::NotUpgradeAuthority));
    assert!(ctx.svm.get_account(&common::config_pda()).is_none());
}
