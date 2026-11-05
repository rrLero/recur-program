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
        solana_pubkey::Pubkey::default(),
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
        solana_pubkey::Pubkey::default(),
    );
    assert!(common::err_is(&result, RecurError::Unauthorized));
    assert_eq!(common::fetch_config(&ctx).admin, ctx.admin.pubkey());
}

#[test]
fn update_config_rejects_fee_over_100_percent() {
    let (mut ctx, treasury) = initialised();
    let admin = ctx.admin.insecure_clone();
    let result = common::update_config(
        &mut ctx,
        &admin,
        admin.pubkey(),
        10_001,
        0,
        treasury,
        false,
        solana_pubkey::Pubkey::default(),
    );
    assert!(common::err_is(&result, RecurError::FeeTooHigh));
}

#[test]
fn update_config_can_hand_admin_over() {
    let (mut ctx, treasury) = initialised();
    let old = ctx.admin.insecure_clone();
    let new = common::funded_keypair(&mut ctx);
    common::update_config(
        &mut ctx,
        &old,
        new.pubkey(),
        200,
        100_000,
        treasury,
        false,
        solana_pubkey::Pubkey::default(),
    )
    .unwrap();
    ctx.svm.expire_blockhash();
    common::update_config(
        &mut ctx,
        &new,
        new.pubkey(),
        300,
        100_000,
        treasury,
        false,
        solana_pubkey::Pubkey::default(),
    )
    .unwrap();
    ctx.svm.expire_blockhash();
    let result = common::update_config(
        &mut ctx,
        &old,
        old.pubkey(),
        400,
        100_000,
        treasury,
        false,
        solana_pubkey::Pubkey::default(),
    );
    assert!(common::err_is(&result, RecurError::Unauthorized));
    assert_eq!(common::fetch_config(&ctx).fee_bps, 300);
}

#[test]
fn update_config_pauses_collect() {
    let (mut ctx, sub) = common::configured_sub(30 * common::USDC, 720);
    common::subscribe(&mut ctx, &sub, None).unwrap();
    common::warp(&mut ctx, 720 * 3_600);
    common::set_paused(&mut ctx);
    assert!(common::fetch_config(&ctx).paused);
    let result = common::collect(&mut ctx, &sub, None);
    assert!(common::err_is(&result, RecurError::Paused));
}

#[test]
fn warp_moves_the_clock_forward() {
    let mut ctx = common::setup();
    let before = common::now(&ctx);
    common::warp(&mut ctx, 3_600);
    assert_eq!(common::now(&ctx), before + 3_600);
}

#[test]
fn sets_the_agent_puller() {
    let (mut ctx, treasury) = initialised();
    let admin = ctx.admin.insecure_clone();
    let puller = common::new_address();
    common::update_config(
        &mut ctx,
        &admin,
        admin.pubkey(),
        200,
        100_000,
        treasury,
        false,
        puller,
    )
    .unwrap();
    let cfg = common::fetch_config(&ctx);
    assert_eq!((cfg.agent_puller, cfg.version), (puller, 2));
}

#[test]
fn reads_a_version_1_config() {
    let (mut ctx, _) = initialised();
    let address = common::ix::config_pda();
    let mut account = ctx.svm.get_account(&address).unwrap();
    account.data[8] = 1;
    let tail = account.data.len() - 64;
    account.data[tail..].fill(0);
    ctx.svm.set_account(address, account).unwrap();
    let cfg = common::fetch_config(&ctx);
    assert_eq!(
        (cfg.version, cfg.agent_puller),
        (1, solana_pubkey::Pubkey::default())
    );
}

#[test]
fn agent_puller_cannot_be_a_recur_pda() {
    let (mut ctx, treasury) = initialised();
    let admin = ctx.admin.insecure_clone();
    for pda in [
        common::ix::config_pda(),
        common::ix::puller_pda(),
        common::ix::transit_pda(),
    ] {
        ctx.svm.expire_blockhash();
        let result = common::update_config(
            &mut ctx,
            &admin,
            admin.pubkey(),
            200,
            100_000,
            treasury,
            false,
            pda,
        );
        assert!(common::err_is(&result, RecurError::Unauthorized));
    }
}
