#![allow(dead_code)]

use anchor_lang::{AccountDeserialize, AccountSerialize, Space};
pub mod ix;

pub use ix::{
    config_pda, membership_pda, merchant_pda, merchant_transit_pda, puller_pda,
    rail_subscription_pda, transit_pda,
};

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

#[allow(clippy::too_many_arguments)]
pub fn update_config(
    ctx: &mut Ctx,
    signer: &Keypair,
    new_admin: Pubkey,
    fee_bps: u16,
    min_fee: u64,
    treasury: Pubkey,
    paused: bool,
    agent_puller: Pubkey,
) -> TxResult {
    let ix = ix::update_config_ix(
        signer.pubkey(),
        new_admin,
        fee_bps,
        min_fee,
        treasury,
        paused,
        agent_puller,
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

pub fn ata(ctx: &mut Ctx, mint: &Pubkey, owner: &Pubkey) -> Pubkey {
    let admin = ctx.admin.insecure_clone();
    CreateAssociatedTokenAccount::new(&mut ctx.svm, &admin, mint)
        .owner(owner)
        .send()
        .unwrap()
}

struct PlanAccess {
    pullers: [Pubkey; 4],
    destinations: [Pubkey; 4],
}

fn create_rail_plan(
    ctx: &mut Ctx,
    merchant: &Keypair,
    mint: &Pubkey,
    plan_id: u64,
    price: u64,
    period_hours: u64,
    access: PlanAccess,
) -> Pubkey {
    let plan = rail_pda(&[b"plan", merchant.pubkey().as_ref(), &plan_id.to_le_bytes()]);
    let create_plan = CreatePlan {
        merchant: merchant.pubkey(),
        plan_pda: plan,
        token_mint: *mint,
        system_program: solana_sdk_ids::system_program::ID,
        token_program: litesvm_token::spl_token::ID,
        payer: None,
    }
    .instruction(CreatePlanInstructionArgs {
        plan_data: PlanData {
            plan_id,
            mint: *mint,
            terms: PlanTerms {
                amount: price,
                period_hours,
                created_at: 0,
            },
            end_ts: 0,
            destinations: access.destinations,
            pullers: access.pullers,
            metadata_uri: [0; 128],
        },
    });
    send(ctx, create_plan, &[merchant]).unwrap();
    plan
}

pub fn second_plan(ctx: &mut Ctx, sub: &Sub) -> Pubkey {
    let merchant = sub.merchant.insecure_clone();
    let price = subscriptions::Plan::from_bytes(&ctx.svm.get_account(&sub.plan).unwrap().data)
        .unwrap()
        .data
        .terms;
    let pullers = [
        puller_pda(),
        Pubkey::default(),
        Pubkey::default(),
        Pubkey::default(),
    ];
    create_rail_plan(
        ctx,
        &merchant,
        &sub.mint,
        3,
        price.amount,
        price.period_hours,
        PlanAccess {
            pullers,
            destinations: default_destinations(&merchant.pubkey()),
        },
    )
}

fn default_destinations(merchant: &Pubkey) -> [Pubkey; 4] {
    [
        *merchant,
        transit_pda(),
        Pubkey::default(),
        Pubkey::default(),
    ]
}

pub fn new_mint(ctx: &mut Ctx) -> Pubkey {
    let admin = ctx.admin.insecure_clone();
    CreateMint::new(&mut ctx.svm, &admin)
        .decimals(6)
        .send()
        .unwrap()
}

pub fn init_subscription_authority_ix(sub: &Sub, payer: Option<Pubkey>) -> Instruction {
    InitSubscriptionAuthority {
        owner: sub.member.pubkey(),
        subscription_authority: sub.subscription_authority,
        token_mint: sub.mint,
        user_ata: sub.member_ata,
        system_program: solana_sdk_ids::system_program::ID,
        token_program: litesvm_token::spl_token::ID,
        payer,
    }
    .instruction()
}

pub fn new_member(ctx: &mut Ctx, sub: &Sub, init_authority: bool) -> Sub {
    let member = funded(ctx);
    let member_ata = ata(ctx, &sub.mint, &member.pubkey());
    let admin = ctx.admin.insecure_clone();
    MintTo::new(&mut ctx.svm, &admin, &sub.mint, &member_ata, 1_000 * USDC)
        .send()
        .unwrap();
    let other = Sub {
        mint: sub.mint,
        subscription_authority: rail_pda(&[
            b"SubscriptionAuthority",
            member.pubkey().as_ref(),
            sub.mint.as_ref(),
        ]),
        subscription: rail_subscription_pda(&sub.plan, &member.pubkey()),
        member,
        merchant: sub.merchant.insecure_clone(),
        referrer: sub.referrer,
        plan: sub.plan,
        member_ata,
        merchant_ata: sub.merchant_ata,
        treasury_ata: sub.treasury_ata,
        referrer_ata: sub.referrer_ata,
        transit_ata: sub.transit_ata,
    };
    if init_authority {
        let member = other.member.insecure_clone();
        send(
            ctx,
            init_subscription_authority_ix(&other, None),
            &[&member],
        )
        .unwrap();
    }
    other
}

pub fn configured_upfront(price: u64, period_hours: u64) -> (Ctx, Sub) {
    let (mut ctx, mut sub) = configured_sub(price, period_hours);
    sub.plan = create_rail_plan(
        &mut ctx,
        &sub.merchant,
        &sub.mint,
        2,
        price,
        period_hours,
        PlanAccess {
            pullers: [Pubkey::default(); 4],
            destinations: default_destinations(&sub.merchant.pubkey()),
        },
    );
    (ctx, sub)
}

pub fn pay_upfront_accounts(
    sub: &Sub,
    referrer_ata: Option<Pubkey>,
) -> recur::accounts::PayUpfront {
    recur::accounts::PayUpfront {
        member: sub.member.pubkey(),
        payer: sub.member.pubkey(),
        config: config_pda(),
        plan: sub.plan,
        merchant: merchant_pda(&sub.merchant.pubkey()),
        membership: membership_pda(&sub.plan, &sub.member.pubkey()),
        mint: sub.mint,
        member_ata: sub.member_ata,
        merchant_ata: sub.merchant_ata,
        treasury_ata: sub.treasury_ata,
        referrer_ata,
        token_program: litesvm_token::spl_token::ID,
        system_program: solana_sdk_ids::system_program::ID,
    }
}

pub fn pay_upfront_with(
    ctx: &mut Ctx,
    sub: &Sub,
    accounts: recur::accounts::PayUpfront,
    referrer: Option<Pubkey>,
) -> TxResult {
    let member = sub.member.insecure_clone();
    send(ctx, ix::pay_upfront_ix(accounts, referrer), &[&member])
}

pub fn pay_upfront(ctx: &mut Ctx, sub: &Sub, referrer: Option<Pubkey>) -> TxResult {
    let accounts = pay_upfront_accounts(sub, referrer.map(|r| ata_of(&sub.mint, &r)));
    pay_upfront_with(ctx, sub, accounts, referrer)
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
    configured(price, period_hours, pullers, false, 1_000 * USDC)
}

pub fn configured_agent_sub(price: u64, period_hours: u64) -> (Ctx, Sub, Keypair) {
    let agent_puller = Keypair::new();
    let pullers = [
        agent_puller.pubkey(),
        puller_pda(),
        Pubkey::default(),
        Pubkey::default(),
    ];
    let (mut ctx, sub) = configured(price, period_hours, pullers, true, 10 * price);
    let cfg = fetch_config(&ctx);
    let admin = ctx.admin.insecure_clone();
    update_config(
        &mut ctx,
        &admin,
        cfg.admin,
        cfg.fee_bps,
        cfg.min_fee,
        cfg.treasury,
        cfg.paused,
        agent_puller.pubkey(),
    )
    .unwrap();
    (ctx, sub, agent_puller)
}

fn configured(
    price: u64,
    period_hours: u64,
    pullers: [Pubkey; 4],
    agent: bool,
    member_funds: u64,
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
    let transit_owner = if agent {
        merchant_transit_pda(&merchant.pubkey())
    } else {
        transit_pda()
    };
    let transit_ata = ata(&mut ctx, &mint, &transit_owner);
    MintTo::new(&mut ctx.svm, &admin, &mint, &member_ata, member_funds)
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

    let destinations = if agent {
        [
            transit_owner,
            Pubkey::default(),
            Pubkey::default(),
            Pubkey::default(),
        ]
    } else {
        default_destinations(&merchant.pubkey())
    };
    let plan = create_rail_plan(
        &mut ctx,
        &merchant,
        &mint,
        1,
        price,
        period_hours,
        PlanAccess {
            pullers,
            destinations,
        },
    );

    let subscription = rail_subscription_pda(&plan, &member.pubkey());

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
    register_merchant(&mut ctx, &sub, if agent { 0 } else { 1_000 }).unwrap();
    (ctx, sub)
}

pub fn rail_subscribe_ix(ctx: &Ctx, sub: &Sub) -> Instruction {
    let sa_account = SubscriptionAuthority::from_bytes(
        &ctx.svm
            .get_account(&sub.subscription_authority)
            .unwrap()
            .data,
    )
    .unwrap();
    rail_subscribe_ix_with(ctx, sub, None, sa_account.init_id)
}

pub fn rail_subscribe_ix_with(
    ctx: &Ctx,
    sub: &Sub,
    payer: Option<Pubkey>,
    authority_init_id: i64,
) -> Instruction {
    let plan_account =
        subscriptions::Plan::from_bytes(&ctx.svm.get_account(&sub.plan).unwrap().data).unwrap();
    Subscribe {
        subscriber: sub.member.pubkey(),
        merchant: sub.merchant.pubkey(),
        plan_pda: sub.plan,
        subscription_pda: sub.subscription,
        subscription_authority_pda: sub.subscription_authority,
        system_program: solana_sdk_ids::system_program::ID,
        event_authority: rail_pda(&[b"event_authority"]),
        self_program: SUBSCRIPTIONS_ID,
        payer,
    }
    .instruction(SubscribeInstructionArgs {
        subscribe_data: SubscribeData {
            plan_id: plan_account.data.plan_id,
            plan_bump: plan_account.bump,
            expected_mint: sub.mint,
            expected_amount: plan_account.data.terms.amount,
            expected_period_hours: plan_account.data.terms.period_hours,
            expected_created_at: plan_account.data.terms.created_at,
            expected_subscription_authority_init_id: authority_init_id,
        },
    })
}

pub fn agent_activation_ixs(ctx: &Ctx, sub: &Sub, agent_puller: &Keypair) -> Vec<Instruction> {
    let price = subscriptions::Plan::from_bytes(&ctx.svm.get_account(&sub.plan).unwrap().data)
        .unwrap()
        .data
        .terms
        .amount;
    let mut ixs = Vec::new();
    if ctx.svm.get_account(&sub.subscription_authority).is_none() {
        ixs.push(init_subscription_authority_ix(sub, None));
    }
    ixs.push(rail_subscribe_ix(ctx, sub));
    ixs.push(
        subscriptions::instructions::TransferSubscription {
            subscription_pda: sub.subscription,
            plan_pda: sub.plan,
            subscription_authority: sub.subscription_authority,
            delegator_ata: sub.member_ata,
            receiver_ata: sub.transit_ata,
            caller: agent_puller.pubkey(),
            token_mint: sub.mint,
            token_program: litesvm_token::spl_token::ID,
            event_authority: Pubkey::find_program_address(
                &[b"event_authority"],
                &subscriptions::SUBSCRIPTIONS_ID,
            )
            .0,
            self_program: subscriptions::SUBSCRIPTIONS_ID,
        }
        .instruction(
            subscriptions::instructions::TransferSubscriptionInstructionArgs {
                transfer_data: subscriptions::types::TransferData {
                    amount: price,
                    delegator: sub.member.pubkey(),
                    mint: sub.mint,
                },
            },
        ),
    );
    ixs
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

pub fn activate_agent_accounts(
    sub: &Sub,
    agent_puller: Pubkey,
    payer: Pubkey,
) -> recur::accounts::ActivateAgent {
    let member = sub.member.pubkey();
    recur::accounts::ActivateAgent {
        agent_puller,
        payer,
        config: config_pda(),
        merchant: merchant_pda(&sub.merchant.pubkey()),
        transit: merchant_transit_pda(&sub.merchant.pubkey()),
        transit_ata: sub.transit_ata,
        mint: sub.mint,
        plan: sub.plan,
        subscription: sub.subscription,
        member,
        membership: membership_pda(&sub.plan, &member),
        merchant_ata: sub.merchant_ata,
        treasury_ata: sub.treasury_ata,
        token_program: litesvm_token::spl_token::ID,
        system_program: solana_sdk_ids::system_program::ID,
    }
}

pub fn activate_agent(ctx: &mut Ctx, sub: &Sub, agent_puller: &Keypair) -> TxResult {
    let payer = funded(ctx);
    let ix = ix::activate_agent_ix(activate_agent_accounts(
        sub,
        agent_puller.pubkey(),
        payer.pubkey(),
    ));
    send(ctx, ix, &[&payer, agent_puller])
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

pub fn grant_comp_ix(sub: &Sub, member: Pubkey, until: i64) -> Instruction {
    let merchant = sub.merchant.pubkey();
    ix::grant_comp_ix(merchant, merchant, sub.plan, member, until)
}

pub fn grant_comp(ctx: &mut Ctx, sub: &Sub, until: i64) -> TxResult {
    let merchant = sub.merchant.insecure_clone();
    send(
        ctx,
        grant_comp_ix(sub, sub.member.pubkey(), until),
        &[&merchant],
    )
}

pub fn revoke_comp_as(ctx: &mut Ctx, sub: &Sub, signer: &Keypair) -> TxResult {
    send(
        ctx,
        ix::revoke_comp_ix(signer.pubkey(), sub.plan, sub.member.pubkey()),
        &[signer],
    )
}

pub fn revoke_comp(ctx: &mut Ctx, sub: &Sub) -> TxResult {
    let merchant = sub.merchant.insecure_clone();
    revoke_comp_as(ctx, sub, &merchant)
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
        cfg.agent_puller,
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

pub fn refund_accounts(sub: &Sub) -> recur::accounts::Refund {
    recur::accounts::Refund {
        owner: sub.merchant.pubkey(),
        membership: membership_pda(&sub.plan, &sub.member.pubkey()),
        owner_ata: sub.merchant_ata,
        member_ata: sub.member_ata,
        token_program: litesvm_token::spl_token::ID,
    }
}

pub fn refund_with(
    ctx: &mut Ctx,
    signer: &Keypair,
    accounts: recur::accounts::Refund,
    amount: u64,
    paid_until: Option<i64>,
) -> TxResult {
    send(ctx, ix::refund_ix(accounts, amount, paid_until), &[signer])
}

pub fn refund(ctx: &mut Ctx, sub: &Sub, amount: u64, paid_until: Option<i64>) -> TxResult {
    let merchant = sub.merchant.insecure_clone();
    refund_with(ctx, &merchant, refund_accounts(sub), amount, paid_until)
}

pub fn close_accounts(sub: &Sub, rent_payer: Pubkey) -> recur::accounts::Close {
    recur::accounts::Close {
        membership: membership_pda(&sub.plan, &sub.member.pubkey()),
        rent_payer,
        subscription: sub.subscription,
    }
}

pub fn close_with(ctx: &mut Ctx, accounts: recur::accounts::Close) -> TxResult {
    let caller = funded_keypair(ctx);
    send(ctx, ix::close_ix(accounts), &[&caller])
}

pub fn close(ctx: &mut Ctx, sub: &Sub) -> TxResult {
    let rent_payer = fetch_membership(ctx, sub).rent_payer;
    close_with(ctx, close_accounts(sub, rent_payer))
}

pub fn cancel_subscription_now(ctx: &mut Ctx, sub: &Sub) -> TxResult {
    let account = ctx.svm.get_account(&sub.subscription).unwrap();
    let subscription = subscriptions::SubscriptionDelegation::from_bytes(&account.data).unwrap();
    let ix = subscriptions::instructions::CancelSubscriptionNow {
        subscriber: sub.member.pubkey(),
        merchant: sub.merchant.pubkey(),
        plan_pda: sub.plan,
        subscription_pda: sub.subscription,
        event_authority: rail_pda(&[b"event_authority"]),
        self_program: SUBSCRIPTIONS_ID,
    }
    .instruction(
        subscriptions::instructions::CancelSubscriptionNowInstructionArgs {
            cancel_subscription_now_data: subscriptions::types::CancelSubscriptionNowData {
                expected_current_period_start_ts: subscription.current_period_start_ts,
            },
        },
    );
    let (member, merchant) = (sub.member.insecure_clone(), sub.merchant.insecure_clone());
    send(ctx, ix, &[&member, &merchant])
}

pub fn anchor_err_is(result: &TxResult, code: u32) -> bool {
    matches!(
        result,
        Err(meta) if matches!(
            meta.err,
            TransactionError::InstructionError(_, InstructionError::Custom(c)) if c == code
        )
    )
}

pub fn abandon_rail_subscription(ctx: &mut Ctx, sub: &Sub) -> TxResult {
    let member = sub.member.insecure_clone();
    let close_sa = subscriptions::instructions::CloseSubscriptionAuthority {
        user: member.pubkey(),
        subscription_authority: sub.subscription_authority,
        receiver: None,
    }
    .instruction();
    send(ctx, close_sa, &[&member])?;
    let mut clock = ctx.svm.get_sysvar::<solana_clock::Clock>();
    clock.slot = clock.slot.checked_add(1).unwrap();
    ctx.svm.set_sysvar(&clock);
    ctx.svm.expire_blockhash();
    let init_sa = InitSubscriptionAuthority {
        owner: member.pubkey(),
        subscription_authority: sub.subscription_authority,
        token_mint: sub.mint,
        user_ata: sub.member_ata,
        system_program: solana_sdk_ids::system_program::ID,
        token_program: litesvm_token::spl_token::ID,
        payer: None,
    }
    .instruction();
    send(ctx, init_sa, &[&member])?;
    let revoke = subscriptions::instructions::RevokeAbandonedSubscription {
        payer: member.pubkey(),
        subscription_account: sub.subscription,
        subscription_authority: sub.subscription_authority,
        plan_pda: sub.plan,
    }
    .instruction();
    send(ctx, revoke, &[&member])
}
