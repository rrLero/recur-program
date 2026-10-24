#![allow(dead_code)]

use anchor_lang::AccountDeserialize;
pub mod ix;

pub use ix::{config_pda, puller_pda, transit_pda};

use litesvm::{types::FailedTransactionMetadata, LiteSVM};
use litesvm_token::{
    get_spl_account, spl_token::state::Account as TokenAccount, CreateAssociatedTokenAccount,
    CreateMint, MintTo,
};
use recur::{Config, RecurError};
use sha2::{Digest, Sha256};
use solana_instruction::{error::InstructionError, Instruction};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;
use solana_transaction_error::TransactionError;
use subscriptions::{
    instructions::{
        CreatePlan, CreatePlanInstructionArgs, InitSubscriptionAuthority, Subscribe,
        SubscribeInstructionArgs,
    },
    types::{PlanData, PlanTerms, SubscribeData},
    SubscriptionAuthority, SUBSCRIPTIONS_ID,
};

pub type TxResult = Result<(), Box<FailedTransactionMetadata>>;

pub struct Ctx {
    pub svm: LiteSVM,
    pub admin: Keypair,
}

pub const USDC: u64 = 1_000_000;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name)
}

fn load_rail(svm: &mut LiteSVM) {
    let bytes = std::fs::read(fixture("tests/fixtures/subscriptions.so")).unwrap();
    let pinned =
        std::fs::read_to_string(fixture("tests/fixtures/subscriptions.so.sha256")).unwrap();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    assert!(
        pinned.starts_with(&hash),
        "rail binary changed; re-pin it (apps/solana/AGENTS.md)"
    );
    svm.add_program(SUBSCRIPTIONS_ID, &bytes).unwrap();
}

pub fn setup() -> Ctx {
    let mut svm = LiteSVM::new();
    svm.add_program_from_file(recur::ID, fixture("../../target/deploy/recur.so"))
        .expect("run `anchor build` first");
    load_rail(&mut svm);
    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    let mut program_data = svm.get_account(&ix::program_data_pda()).unwrap();
    program_data.data[12] = 1;
    program_data.data[13..45].copy_from_slice(admin.pubkey().as_ref());
    svm.set_account(ix::program_data_pda(), program_data)
        .unwrap();
    Ctx { svm, admin }
}

pub fn new_address() -> Pubkey {
    Pubkey::new_unique()
}

pub fn send(ctx: &mut Ctx, ix: Instruction, signers: &[&Keypair]) -> TxResult {
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&signers[0].pubkey()),
        signers,
        ctx.svm.latest_blockhash(),
    );
    ctx.svm.send_transaction(tx).map(|_| ()).map_err(Box::new)
}

pub fn init_config(ctx: &mut Ctx, fee_bps: u16, min_fee: u64, treasury: Pubkey) -> TxResult {
    let admin = ctx.admin.insecure_clone();
    init_config_as(ctx, &admin, fee_bps, min_fee, treasury)
}

pub fn init_config_as(
    ctx: &mut Ctx,
    signer: &Keypair,
    fee_bps: u16,
    min_fee: u64,
    treasury: Pubkey,
) -> TxResult {
    let ix = ix::init_config_ix(signer.pubkey(), fee_bps, min_fee, treasury);
    send(ctx, ix, &[signer])
}

pub fn update_config(
    ctx: &mut Ctx,
    signer: &Keypair,
    new_admin: Pubkey,
    fee_bps: u16,
    min_fee: u64,
    treasury: Pubkey,
    paused: bool,
) -> TxResult {
    let ix = ix::update_config_ix(signer.pubkey(), new_admin, fee_bps, min_fee, treasury, paused);
    send(ctx, ix, &[signer])
}

pub fn sweep_transit(
    ctx: &mut Ctx,
    signer: &Keypair,
    transit_ata: Pubkey,
    treasury_ata: Pubkey,
) -> TxResult {
    let ix = ix::sweep_transit_ix(signer.pubkey(), transit_ata, treasury_ata);
    send(ctx, ix, &[signer])
}

pub fn funded_keypair(ctx: &mut Ctx) -> Keypair {
    funded(ctx)
}

pub fn warp(ctx: &mut Ctx, secs: i64) {
    let mut clock = ctx.svm.get_sysvar::<solana_clock::Clock>();
    clock.unix_timestamp = clock.unix_timestamp.checked_add(secs).unwrap();
    ctx.svm.set_sysvar(&clock);
}

pub fn now(ctx: &Ctx) -> i64 {
    ctx.svm.get_sysvar::<solana_clock::Clock>().unix_timestamp
}

pub fn fetch_config(ctx: &Ctx) -> Config {
    let account = ctx.svm.get_account(&config_pda()).expect("config exists");
    Config::try_deserialize(&mut account.data.as_slice()).unwrap()
}

pub fn err_is(result: &TxResult, expected: RecurError) -> bool {
    matches!(
        result,
        Err(meta) if meta.err == TransactionError::InstructionError(0, InstructionError::Custom(u32::from(expected)))
    )
}

pub struct Sub {
    pub mint: Pubkey,
    pub member: Keypair,
    pub merchant: Keypair,
    pub referrer: Pubkey,
    pub plan: Pubkey,
    pub subscription: Pubkey,
    pub subscription_authority: Pubkey,
    pub member_ata: Pubkey,
    pub merchant_ata: Pubkey,
    pub treasury_ata: Pubkey,
    pub referrer_ata: Pubkey,
    pub transit_ata: Pubkey,
}

fn rail_pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &SUBSCRIPTIONS_ID).0
}

fn funded(ctx: &mut Ctx) -> Keypair {
    let kp = Keypair::new();
    ctx.svm.airdrop(&kp.pubkey(), 1_000_000_000).unwrap();
    kp
}

fn ata(ctx: &mut Ctx, mint: &Pubkey, owner: &Pubkey) -> Pubkey {
    let admin = ctx.admin.insecure_clone();
    CreateAssociatedTokenAccount::new(&mut ctx.svm, &admin, mint)
        .owner(owner)
        .send()
        .unwrap()
}

pub fn configured_sub(price: u64, period_hours: u64) -> (Ctx, Sub) {
    configured_sub_with_pullers(
        price,
        period_hours,
        [
            puller_pda(),
            Pubkey::default(),
            Pubkey::default(),
            Pubkey::default(),
        ],
    )
}

pub fn configured_sub_with_pullers(
    price: u64,
    period_hours: u64,
    pullers: [Pubkey; 4],
) -> (Ctx, Sub) {
    let mut ctx = setup();
    let treasury = new_address();
    init_config(&mut ctx, 200, 100_000, treasury).unwrap();
    let admin = ctx.admin.insecure_clone();
    let mint = CreateMint::new(&mut ctx.svm, &admin)
        .decimals(6)
        .send()
        .unwrap();
    let member = funded(&mut ctx);
    let merchant = funded(&mut ctx);
    let referrer = new_address();
    let member_ata = ata(&mut ctx, &mint, &member.pubkey());
    let merchant_ata = ata(&mut ctx, &mint, &merchant.pubkey());
    let treasury_ata = ata(&mut ctx, &mint, &treasury);
    let referrer_ata = ata(&mut ctx, &mint, &referrer);
    let transit_ata = ata(&mut ctx, &mint, &transit_pda());
    MintTo::new(&mut ctx.svm, &admin, &mint, &member_ata, 1_000 * USDC)
        .send()
        .unwrap();

    let subscription_authority = rail_pda(&[
        b"SubscriptionAuthority",
        member.pubkey().as_ref(),
        mint.as_ref(),
    ]);
    let init_sa = InitSubscriptionAuthority {
        owner: member.pubkey(),
        subscription_authority,
        token_mint: mint,
        user_ata: member_ata,
        system_program: solana_sdk_ids::system_program::ID,
        token_program: litesvm_token::spl_token::ID,
        payer: None,
    }
    .instruction();
    send(&mut ctx, init_sa, &[&member]).unwrap();

    let plan_id: u64 = 1;
    let plan = rail_pda(&[b"plan", merchant.pubkey().as_ref(), &plan_id.to_le_bytes()]);
    let create_plan = CreatePlan {
        merchant: merchant.pubkey(),
        plan_pda: plan,
        token_mint: mint,
        system_program: solana_sdk_ids::system_program::ID,
        token_program: litesvm_token::spl_token::ID,
        payer: None,
    }
    .instruction(CreatePlanInstructionArgs {
        plan_data: PlanData {
            plan_id,
            mint,
            terms: PlanTerms {
                amount: price,
                period_hours,
                created_at: 0,
            },
            end_ts: 0,
            destinations: [
                merchant.pubkey(),
                transit_pda(),
                Pubkey::default(),
                Pubkey::default(),
            ],
            pullers,
            metadata_uri: [0; 128],
        },
    });
    send(&mut ctx, create_plan, &[&merchant]).unwrap();

    let plan_account =
        subscriptions::Plan::from_bytes(&ctx.svm.get_account(&plan).unwrap().data).unwrap();
    let sa_account = SubscriptionAuthority::from_bytes(
        &ctx.svm.get_account(&subscription_authority).unwrap().data,
    )
    .unwrap();
    let subscription = rail_pda(&[b"subscription", plan.as_ref(), member.pubkey().as_ref()]);
    let subscribe = Subscribe {
        subscriber: member.pubkey(),
        merchant: merchant.pubkey(),
        plan_pda: plan,
        subscription_pda: subscription,
        subscription_authority_pda: subscription_authority,
        system_program: solana_sdk_ids::system_program::ID,
        event_authority: rail_pda(&[b"event_authority"]),
        self_program: SUBSCRIPTIONS_ID,
        payer: None,
    }
    .instruction(SubscribeInstructionArgs {
        subscribe_data: SubscribeData {
            plan_id,
            plan_bump: plan_account.bump,
            expected_mint: mint,
            expected_amount: price,
            expected_period_hours: period_hours,
            expected_created_at: plan_account.data.terms.created_at,
            expected_subscription_authority_init_id: sa_account.init_id,
        },
    });
    send(&mut ctx, subscribe, &[&member]).unwrap();

    let sub = Sub {
        mint,
        member,
        merchant,
        referrer,
        plan,
        subscription,
        subscription_authority,
        member_ata,
        merchant_ata,
        treasury_ata,
        referrer_ata,
        transit_ata,
    };
    (ctx, sub)
}

pub fn collect(ctx: &mut Ctx, sub: &Sub, amount: u64, referral: Option<u16>) -> TxResult {
    let payer = funded(ctx);
    let accounts = recur::accounts::Collect {
        caller: payer.pubkey(),
        config: config_pda(),
        puller: puller_pda(),
        transit: transit_pda(),
        transit_ata: sub.transit_ata,
        mint: sub.mint,
        plan: sub.plan,
        subscription: sub.subscription,
        subscription_authority: sub.subscription_authority,
        member_ata: sub.member_ata,
        merchant_ata: sub.merchant_ata,
        treasury_ata: sub.treasury_ata,
        referrer_ata: referral.map(|_| sub.referrer_ata),
        event_authority: rail_pda(&[b"event_authority"]),
        rail_program: SUBSCRIPTIONS_ID,
        token_program: litesvm_token::spl_token::ID,
    };
    let ix = ix::collect_ix(accounts, amount, referral.unwrap_or(0));
    send(ctx, ix, &[&payer])
}

pub fn balance(ctx: &Ctx, token_account: Pubkey) -> u64 {
    get_spl_account::<TokenAccount>(&ctx.svm, &token_account)
        .unwrap()
        .amount
}

pub fn set_paused(ctx: &mut Ctx) {
    let cfg = fetch_config(ctx);
    let admin = ctx.admin.insecure_clone();
    update_config(
        ctx,
        &admin,
        cfg.admin,
        cfg.fee_bps,
        cfg.min_fee,
        cfg.treasury,
        true,
    )
    .unwrap();
}

pub fn drain_member(ctx: &mut Ctx, sub: &Sub) {
    let mut account = ctx.svm.get_account(&sub.member_ata).unwrap();
    let mut token = get_spl_account::<TokenAccount>(&ctx.svm, &sub.member_ata).unwrap();
    token.amount = 0;
    solana_program_pack::Pack::pack(token, &mut account.data).unwrap();
    ctx.svm.set_account(sub.member_ata, account).unwrap();
}
