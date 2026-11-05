mod common;

use recur::RecurError;
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
fn init_config_rejects_fee_over_100_percent() {
    let mut ctx = common::setup();
    let result = common::init_config(&mut ctx, 10_001, 0, common::new_address());
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
