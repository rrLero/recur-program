#![allow(dead_code)]

use anchor_lang::{AccountDeserialize, AccountSerialize, Space};
pub mod ix;

pub use ix::{config_pda, membership_pda, merchant_pda, puller_pda, transit_pda};

use litesvm::{types::FailedTransactionMetadata, LiteSVM};
use litesvm_token::{
    get_spl_account, spl_token::state::Account as TokenAccount, CreateAssociatedTokenAccount,
    CreateMint, MintTo,
};
use recur::{Config, Membership, Merchant, RecurError};
use sha2::{Digest, Sha256};
use solana_instruction::{error::InstructionError, Instruction};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;
use solana_transaction_error::TransactionError;
use subscriptions::{
    instructions::{
        CreatePlan, CreatePlanInstructionArgs, InitSubscriptionAuthority,
        RevokeSubscriptionAuthority, Subscribe, SubscribeInstructionArgs,
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
pub const GENESIS_TS: i64 = 1_761_264_000;

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
    let mut clock = svm.get_sysvar::<solana_clock::Clock>();
    clock.unix_timestamp = GENESIS_TS;
    svm.set_sysvar(&clock);
    Ctx { svm, admin }
}

pub fn new_address() -> Pubkey {
    Pubkey::new_unique()
}

pub fn send(ctx: &mut Ctx, ix: Instruction, signers: &[&Keypair]) -> TxResult {
    send_many(ctx, &[ix], signers)
}

pub fn send_many(ctx: &mut Ctx, ixs: &[Instruction], signers: &[&Keypair]) -> TxResult {
    let tx = Transaction::new_signed_with_payer(
        ixs,
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
    let ix = ix::update_config_ix(
        signer.pubkey(),
        new_admin,
        fee_bps,
        min_fee,
        treasury,
        paused,
    );
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

pub fn register_merchant(ctx: &mut Ctx, sub: &Sub, referral_bps: u16) -> TxResult {
    let owner = sub.merchant.insecure_clone();
    register_merchant_as(ctx, &owner, referral_bps)
}

pub fn register_merchant_as(ctx: &mut Ctx, owner: &Keypair, referral_bps: u16) -> TxResult {
    let ix = ix::register_merchant_ix(owner.pubkey(), owner.pubkey(), referral_bps);
    send(ctx, ix, &[owner])
}

pub fn update_merchant(
    ctx: &mut Ctx,
    signer: &Keypair,
    merchant_owner: Pubkey,
    referral_bps: u16,
) -> TxResult {
    let ix = ix::update_merchant_ix(signer.pubkey(), merchant_owner, referral_bps);
    send(ctx, ix, &[signer])
}

pub fn set_merchant_terms(
    ctx: &mut Ctx,
    signer: &Keypair,
    merchant_owner: Pubkey,
    fee_bps_override: Option<u16>,
    fee_free_until: i64,
) -> TxResult {
    let ix = ix::set_merchant_terms_ix(
        signer.pubkey(),
        merchant_owner,
        fee_bps_override,
        fee_free_until,
    );
    send(ctx, ix, &[signer])
}

pub fn fetch_merchant(ctx: &Ctx, owner: Pubkey) -> Merchant {
    let account = ctx
        .svm
        .get_account(&merchant_pda(&owner))
        .expect("merchant exists");
    Merchant::try_deserialize(&mut account.data.as_slice()).unwrap()
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
    err_at(result, 0, expected)
}

pub fn err_at(result: &TxResult, index: u8, expected: RecurError) -> bool {
    matches!(
        result,
        Err(meta) if meta.err == TransactionError::InstructionError(index, InstructionError::Custom(u32::from(expected)))
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

    let subscription = rail_pda(&[b"subscription", plan.as_ref(), member.pubkey().as_ref()]);

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
    register_merchant(&mut ctx, &sub, 1_000).unwrap();
    (ctx, sub)
}

pub fn rail_subscribe_ix(ctx: &Ctx, sub: &Sub) -> Instruction {
    let plan_account =
        subscriptions::Plan::from_bytes(&ctx.svm.get_account(&sub.plan).unwrap().data).unwrap();
    let sa_account = SubscriptionAuthority::from_bytes(
        &ctx.svm
            .get_account(&sub.subscription_authority)
            .unwrap()
            .data,
    )
    .unwrap();
    Subscribe {
        subscriber: sub.member.pubkey(),
        merchant: sub.merchant.pubkey(),
        plan_pda: sub.plan,
        subscription_pda: sub.subscription,
        subscription_authority_pda: sub.subscription_authority,
        system_program: solana_sdk_ids::system_program::ID,
        event_authority: rail_pda(&[b"event_authority"]),
        self_program: SUBSCRIPTIONS_ID,
        payer: None,
    }
    .instruction(SubscribeInstructionArgs {
        subscribe_data: SubscribeData {
            plan_id: plan_account.data.plan_id,
            plan_bump: plan_account.bump,
            expected_mint: sub.mint,
            expected_amount: plan_account.data.terms.amount,
            expected_period_hours: plan_account.data.terms.period_hours,
            expected_created_at: plan_account.data.terms.created_at,
            expected_subscription_authority_init_id: sa_account.init_id,
        },
    })
}

pub fn collect_accounts(sub: &Sub, referrer_ata: Option<Pubkey>) -> recur::accounts::Collect {
    recur::accounts::Collect {
        config: config_pda(),
        merchant: merchant_pda(&sub.merchant.pubkey()),
        membership: membership_pda(&sub.plan, &sub.member.pubkey()),
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
        referrer_ata,
        event_authority: rail_pda(&[b"event_authority"]),
        rail_program: SUBSCRIPTIONS_ID,
        token_program: litesvm_token::spl_token::ID,
    }
}

pub fn collect(ctx: &mut Ctx, sub: &Sub, referrer_ata: Option<Pubkey>) -> TxResult {
    let payer = funded(ctx);
    let ix = ix::collect_ix(collect_accounts(sub, referrer_ata));
    send(ctx, ix, &[&payer])
}

pub fn ata_of(mint: &Pubkey, owner: &Pubkey) -> Pubkey {
    anchor_spl::associated_token::get_associated_token_address(owner, mint)
}

pub fn subscribe_hook_ix(sub: &Sub, referrer: Option<Pubkey>) -> Instruction {
    let member = sub.member.pubkey();
    ix::subscribe_hook_ix(member, member, sub.plan, sub.merchant.pubkey(), referrer)
}

pub fn subscribe_hook(ctx: &mut Ctx, sub: &Sub, referrer: Option<Pubkey>) -> TxResult {
    let member = sub.member.insecure_clone();
    let ix = subscribe_hook_ix(sub, referrer);
    send(ctx, ix, &[&member])
}

pub fn renew(ctx: &mut Ctx, sub: &Sub, referrer: Option<Pubkey>) -> TxResult {
    let member = sub.member.insecure_clone();
    let ixs = [
        subscribe_hook_ix(sub, referrer),
        ix::collect_ix(collect_accounts(
            sub,
            referrer.map(|r| ata_of(&sub.mint, &r)),
        )),
    ];
    send_many(ctx, &ixs, &[&member])
}

pub fn subscribe(ctx: &mut Ctx, sub: &Sub, referrer: Option<Pubkey>) -> TxResult {
    let member = sub.member.insecure_clone();
    let ixs = [
        rail_subscribe_ix(ctx, sub),
        subscribe_hook_ix(sub, referrer),
        ix::collect_ix(collect_accounts(
            sub,
            referrer.map(|r| ata_of(&sub.mint, &r)),
        )),
    ];
    send_many(ctx, &ixs, &[&member])
}

pub fn rail_subscribe(ctx: &mut Ctx, sub: &Sub) -> TxResult {
    let member = sub.member.insecure_clone();
    let ix = rail_subscribe_ix(ctx, sub);
    send(ctx, ix, &[&member])
}

pub fn try_fetch_membership(ctx: &Ctx, sub: &Sub) -> Option<Membership> {
    let account = ctx
        .svm
        .get_account(&membership_pda(&sub.plan, &sub.member.pubkey()))?;
    Membership::try_deserialize(&mut account.data.as_slice()).ok()
}

pub fn fetch_membership(ctx: &Ctx, sub: &Sub) -> Membership {
    try_fetch_membership(ctx, sub).expect("membership exists")
}

pub fn paid_until(ctx: &Ctx, sub: &Sub) -> Option<i64> {
    try_fetch_membership(ctx, sub).map(|m| m.paid_until)
}

pub fn write_membership(ctx: &mut Ctx, membership: &Membership) {
    let address = membership_pda(&membership.plan, &membership.member);
    let mut membership = membership.clone();
    membership.bump = Pubkey::find_program_address(
        &[
            recur::MEMBERSHIP_SEED,
            membership.plan.as_ref(),
            membership.member.as_ref(),
        ],
        &recur::ID,
    )
    .1;
    let mut data = Vec::new();
    membership.try_serialize(&mut data).unwrap();
    data.resize(8 + Membership::INIT_SPACE, 0);
    let lamports = ctx.svm.minimum_balance_for_rent_exemption(data.len());
    ctx.svm
        .set_account(
            address,
            solana_account::Account {
                lamports,
                data,
                owner: recur::ID,
                executable: false,
                rent_epoch: 0,
            },
        )
        .unwrap();
}

pub fn token_account(ctx: &mut Ctx, sub: &Sub, owner: &Pubkey) -> Pubkey {
    let mint = sub.mint;
    ata(ctx, &mint, owner)
}

pub fn fund_member(ctx: &mut Ctx, sub: &Sub, amount: u64) {
    let admin = ctx.admin.insecure_clone();
    MintTo::new(&mut ctx.svm, &admin, &sub.mint, &sub.member_ata, amount)
        .send()
        .unwrap();
}

pub fn set_plan(ctx: &mut Ctx, sub: &Sub, edit: impl FnOnce(&mut subscriptions::Plan)) {
    let mut account = ctx.svm.get_account(&sub.plan).unwrap();
    let mut plan = subscriptions::Plan::from_bytes(&account.data).unwrap();
    edit(&mut plan);
    let bytes = borsh::to_vec(&plan).unwrap();
    account.data[..bytes.len()].copy_from_slice(&bytes);
    ctx.svm.set_account(sub.plan, account).unwrap();
}

pub fn revoke_delegate(ctx: &mut Ctx, sub: &Sub) -> TxResult {
    let member = sub.member.insecure_clone();
    let ix = RevokeSubscriptionAuthority {
        user: member.pubkey(),
        user_ata: sub.member_ata,
        token_mint: sub.mint,
        token_program: litesvm_token::spl_token::ID,
        subscription_authority: sub.subscription_authority,
        receiver: None,
    }
    .instruction();
    send(ctx, ix, &[&member])
}

pub fn close_member_ata(ctx: &mut Ctx, sub: &Sub) {
    drain_member(ctx, sub);
    let member = sub.member.insecure_clone();
    let ix = litesvm_token::spl_token::instruction::close_account(
        &litesvm_token::spl_token::ID,
        &sub.member_ata,
        &member.pubkey(),
        &member.pubkey(),
        &[],
    )
    .unwrap();
    send(ctx, ix, &[&member]).unwrap();
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
