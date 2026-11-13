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

#[test]
fn handover_leaves_a_paused_config_owned_by_the_vault() {
    let mut ctx = common::setup();
    let deployer = ctx.admin.insecure_clone();
    let mint = litesvm_token::CreateMint::new(&mut ctx.svm, &deployer)
        .decimals(6)
        .send()
        .unwrap();
    let vault = common::ix::squads_vault_pda(&common::new_address());
    let agent_puller = common::new_address();
    let ixs = common::ix::handover_ixs(&common::ix::Handover {
        deployer: deployer.pubkey(),
        vault,
        mint,
        agent_puller,
        fee_bps: 200,
        min_fee: 100_000,
    });
    common::send_many(&mut ctx, &ixs, &[&deployer]).unwrap();

    let cfg = common::fetch_config(&ctx);
    assert_eq!(
        (cfg.admin, cfg.treasury, cfg.agent_puller, cfg.paused),
        (vault, vault, agent_puller, true)
    );
    assert_eq!((cfg.fee_bps, cfg.min_fee), (200, 100_000));
    let vault_ata = ctx
        .svm
        .get_account(&common::ix::ata_of(&mint, &vault))
        .unwrap();
    assert_eq!(vault_ata.owner, anchor_spl::token::ID);

    let result = common::update_config(
        &mut ctx,
        &deployer,
        deployer.pubkey(),
        200,
        100_000,
        vault,
        false,
        agent_puller,
    );
    assert!(common::err_is(&result, RecurError::Unauthorized));
}
