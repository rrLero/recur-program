#[path = "../tests/common/ix.rs"]
mod ix;

use std::{env, fs, path::PathBuf};

use anchor_lang::AccountDeserialize;
use litesvm_token::spl_token;
use recur::Config;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::{read_keypair_file, write_keypair_file, Keypair};
use solana_program_pack::Pack;
use solana_pubkey::Pubkey;
use solana_rpc_client::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_system_interface::instruction as system;
use solana_transaction::Transaction;
use subscriptions::{
    instructions::{
        CreatePlan, CreatePlanInstructionArgs, InitSubscriptionAuthority, Subscribe,
        SubscribeInstructionArgs,
    },
    types::{PlanData, PlanTerms, SubscribeData},
    Plan, SubscriptionAuthority, SUBSCRIPTIONS_ID,
};

const RPC_URL: &str = "https://api.devnet.solana.com";
const USDC: u64 = 1_000_000;
const PLAN_ID: u64 = 1;
const ASSOCIATED_TOKEN_PROGRAM: Pubkey =
    Pubkey::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn state_dir() -> Res<PathBuf> {
    let dir = PathBuf::from(env::var("HOME")?).join(".config/recur-devnet");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn persistent_keypair(name: &str) -> Res<Keypair> {
    let path = state_dir()?.join(name);
    if path.exists() {
        return read_keypair_file(&path);
    }
    let keypair = Keypair::new();
    write_keypair_file(&keypair, &path)?;
    Ok(keypair)
}

fn rail_pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &SUBSCRIPTIONS_ID).0
}

fn ata_of(mint: &Pubkey, owner: &Pubkey) -> Pubkey {
    anchor_spl::associated_token::get_associated_token_address(owner, mint)
}

fn create_ata_ix(payer: &Pubkey, owner: &Pubkey, mint: &Pubkey) -> Instruction {
    Instruction {
        program_id: ASSOCIATED_TOKEN_PROGRAM,
        accounts: vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(ata_of(mint, owner), false),
            AccountMeta::new_readonly(*owner, false),
            AccountMeta::new_readonly(*mint, false),
            AccountMeta::new_readonly(solana_sdk_ids::system_program::ID, false),
            AccountMeta::new_readonly(spl_token::ID, false),
        ],
        data: vec![1],
    }
}

fn send(rpc: &RpcClient, payer: &Keypair, ixs: &[Instruction], extra: &[&Keypair]) -> Res<String> {
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend(extra);
    let tx = Transaction::new_signed_with_payer(
        ixs,
        Some(&payer.pubkey()),
        &signers,
        rpc.get_latest_blockhash()?,
    );
    Ok(rpc.send_and_confirm_transaction(&tx)?.to_string())
}

fn exists(rpc: &RpcClient, address: &Pubkey) -> Res<bool> {
    Ok(rpc
        .get_account_with_commitment(address, rpc.commitment())?
        .value
        .is_some())
}

fn token_balance(rpc: &RpcClient, token_account: &Pubkey) -> Res<u64> {
    Ok(rpc
        .get_token_account_balance(token_account)?
        .amount
        .parse()?)
}

struct Setup {
    mint: Pubkey,
    member: Keypair,
    merchant: Keypair,
    treasury: Pubkey,
    plan: Pubkey,
    subscription: Pubkey,
    subscription_authority: Pubkey,
}

impl Setup {
    fn member_ata(&self) -> Pubkey {
        ata_of(&self.mint, &self.member.pubkey())
    }
    fn merchant_ata(&self) -> Pubkey {
        ata_of(&self.mint, &self.merchant.pubkey())
    }
    fn treasury_ata(&self) -> Pubkey {
        ata_of(&self.mint, &self.treasury)
    }
    fn transit_ata(&self) -> Pubkey {
        ata_of(&self.mint, &ix::transit_pda())
    }

    fn collect_ix(&self) -> Instruction {
        ix::collect_ix(recur::accounts::Collect {
            config: ix::config_pda(),
            merchant: ix::merchant_pda(&self.merchant.pubkey()),
            membership: ix::membership_pda(&self.plan, &self.member.pubkey()),
            puller: ix::puller_pda(),
            transit: ix::transit_pda(),
            transit_ata: self.transit_ata(),
            mint: self.mint,
            plan: self.plan,
            subscription: self.subscription,
            subscription_authority: self.subscription_authority,
            member_ata: self.member_ata(),
            merchant_ata: self.merchant_ata(),
            treasury_ata: self.treasury_ata(),
            referrer_ata: None,
            event_authority: rail_pda(&[b"event_authority"]),
            rail_program: SUBSCRIPTIONS_ID,
            token_program: spl_token::ID,
        })
    }
}

fn print_balances(rpc: &RpcClient, setup: &Setup) -> Res<()> {
    for (label, account) in [
        ("member", setup.member_ata()),
        ("merchant", setup.merchant_ata()),
        ("treasury", setup.treasury_ata()),
        ("transit", setup.transit_ata()),
    ] {
        println!(
            "{label}: {:.6} USDC",
            token_balance(rpc, &account)? as f64 / USDC as f64
        );
    }
    Ok(())
}

fn ensure_config(rpc: &RpcClient, wallet: &Keypair) -> Res<Pubkey> {
    let config_address = ix::config_pda();
    if !exists(rpc, &config_address)? {
        let init = ix::init_config_ix(wallet.pubkey(), 200, 100_000, wallet.pubkey());
        println!("init_config: {}", send(rpc, wallet, &[init], &[])?);
    }
    let data = rpc.get_account(&config_address)?.data;
    let config = Config::try_deserialize(&mut data.as_slice())?;
    assert_eq!(
        config.admin,
        wallet.pubkey(),
        "config.admin is not the wallet"
    );
    Ok(config.treasury)
}

fn ensure_funded(rpc: &RpcClient, wallet: &Keypair, to: &Pubkey, lamports: u64) -> Res<()> {
    if rpc.get_balance(to)? < lamports {
        send(
            rpc,
            wallet,
            &[system::transfer(&wallet.pubkey(), to, lamports)],
            &[],
        )?;
    }
    Ok(())
}

fn ensure_mint(rpc: &RpcClient, wallet: &Keypair, mint: &Keypair) -> Res<()> {
    if exists(rpc, &mint.pubkey())? {
        return Ok(());
    }
    let rent = rpc.get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)?;
    let ixs = [
        system::create_account(
            &wallet.pubkey(),
            &mint.pubkey(),
            rent,
            spl_token::state::Mint::LEN as u64,
            &spl_token::ID,
        ),
        spl_token::instruction::initialize_mint2(
            &spl_token::ID,
            &mint.pubkey(),
            &wallet.pubkey(),
            None,
            6,
        )?,
    ];
    send(rpc, wallet, &ixs, &[mint])?;
    Ok(())
}

fn ensure_atas(rpc: &RpcClient, wallet: &Keypair, setup: &Setup) -> Res<()> {
    let owners = [
        setup.member.pubkey(),
        setup.merchant.pubkey(),
        setup.treasury,
        ix::transit_pda(),
    ];
    let ixs: Vec<Instruction> = owners
        .iter()
        .map(|owner| create_ata_ix(&wallet.pubkey(), owner, &setup.mint))
        .collect();
    send(rpc, wallet, &ixs, &[])?;
    Ok(())
}

fn ensure_merchant_and_plan(rpc: &RpcClient, wallet: &Keypair, setup: &Setup) -> Res<()> {
    let merchant = setup.merchant.pubkey();
    if !exists(rpc, &ix::merchant_pda(&merchant))? {
        let register = ix::register_merchant_ix(merchant, wallet.pubkey(), 0);
        send(rpc, wallet, &[register], &[&setup.merchant])?;
    }
    if !exists(rpc, &setup.plan)? {
        let create_plan = CreatePlan {
            merchant,
            plan_pda: setup.plan,
            token_mint: setup.mint,
            system_program: solana_sdk_ids::system_program::ID,
            token_program: spl_token::ID,
            payer: Some(wallet.pubkey()),
        }
        .instruction(CreatePlanInstructionArgs {
            plan_data: PlanData {
                plan_id: PLAN_ID,
                mint: setup.mint,
                terms: PlanTerms {
                    amount: USDC,
                    period_hours: 1,
                    created_at: 0,
                },
                end_ts: 0,
                destinations: [
                    merchant,
                    ix::transit_pda(),
                    Pubkey::default(),
                    Pubkey::default(),
                ],
                pullers: [
                    ix::puller_pda(),
                    Pubkey::default(),
                    Pubkey::default(),
                    Pubkey::default(),
                ],
                metadata_uri: [0; 128],
            },
        });
        send(rpc, wallet, &[create_plan], &[&setup.merchant])?;
    }
    Ok(())
}

fn ensure_subscription_authority(rpc: &RpcClient, wallet: &Keypair, setup: &Setup) -> Res<()> {
    if exists(rpc, &setup.subscription_authority)? {
        return Ok(());
    }
    let init = InitSubscriptionAuthority {
        owner: setup.member.pubkey(),
        subscription_authority: setup.subscription_authority,
        token_mint: setup.mint,
        user_ata: setup.member_ata(),
        system_program: solana_sdk_ids::system_program::ID,
        token_program: spl_token::ID,
        payer: Some(wallet.pubkey()),
    }
    .instruction();
    send(rpc, wallet, &[init], &[&setup.member])?;
    Ok(())
}

fn subscribe_bundle(rpc: &RpcClient, wallet: &Keypair, setup: &Setup) -> Res<String> {
    let plan = Plan::from_bytes(&rpc.get_account(&setup.plan)?.data)?;
    let authority =
        SubscriptionAuthority::from_bytes(&rpc.get_account(&setup.subscription_authority)?.data)?;
    let rail_subscribe = Subscribe {
        subscriber: setup.member.pubkey(),
        merchant: setup.merchant.pubkey(),
        plan_pda: setup.plan,
        subscription_pda: setup.subscription,
        subscription_authority_pda: setup.subscription_authority,
        system_program: solana_sdk_ids::system_program::ID,
        event_authority: rail_pda(&[b"event_authority"]),
        self_program: SUBSCRIPTIONS_ID,
        payer: Some(wallet.pubkey()),
    }
    .instruction(SubscribeInstructionArgs {
        subscribe_data: SubscribeData {
            plan_id: plan.data.plan_id,
            plan_bump: plan.bump,
            expected_mint: setup.mint,
            expected_amount: plan.data.terms.amount,
            expected_period_hours: plan.data.terms.period_hours,
            expected_created_at: plan.data.terms.created_at,
            expected_subscription_authority_init_id: authority.init_id,
        },
    });
    let hook = ix::subscribe_hook_ix(
        setup.member.pubkey(),
        wallet.pubkey(),
        setup.plan,
        setup.merchant.pubkey(),
        None,
    );
    send(
        rpc,
        wallet,
        &[rail_subscribe, hook, setup.collect_ix()],
        &[&setup.member],
    )
}

fn main() -> Res<()> {
    let mut args = env::args().skip(1);
    let wallet_path = args
        .next()
        .ok_or("usage: devnet_collect <wallet path> [--resume]")?;
    let resume = args.next().as_deref() == Some("--resume");

    let wallet = read_keypair_file(&wallet_path)?;
    let rpc = RpcClient::new(RPC_URL.to_string());
    let mint = persistent_keypair("mint.json")?;
    let member = persistent_keypair("member.json")?;
    let merchant = persistent_keypair("merchant.json")?;

    let treasury = ensure_config(&rpc, &wallet)?;
    let plan = rail_pda(&[b"plan", merchant.pubkey().as_ref(), &PLAN_ID.to_le_bytes()]);
    let setup = Setup {
        mint: mint.pubkey(),
        subscription: rail_pda(&[b"subscription", plan.as_ref(), member.pubkey().as_ref()]),
        subscription_authority: rail_pda(&[
            b"SubscriptionAuthority",
            member.pubkey().as_ref(),
            mint.pubkey().as_ref(),
        ]),
        member,
        merchant,
        treasury,
        plan,
    };

    if resume {
        let signature = send(&rpc, &wallet, &[setup.collect_ix()], &[])?;
        println!("collect: {signature}");
        return print_balances(&rpc, &setup);
    }

    ensure_funded(&rpc, &wallet, &setup.member.pubkey(), 50_000_000)?;
    ensure_funded(&rpc, &wallet, &setup.merchant.pubkey(), 50_000_000)?;
    ensure_mint(&rpc, &wallet, &mint)?;
    ensure_atas(&rpc, &wallet, &setup)?;
    let mint_to = spl_token::instruction::mint_to(
        &spl_token::ID,
        &setup.mint,
        &setup.member_ata(),
        &wallet.pubkey(),
        &[],
        10 * USDC,
    )?;
    send(&rpc, &wallet, &[mint_to], &[])?;
    ensure_merchant_and_plan(&rpc, &wallet, &setup)?;
    ensure_subscription_authority(&rpc, &wallet, &setup)?;

    println!(
        "subscribe bundle: {}",
        subscribe_bundle(&rpc, &wallet, &setup)?
    );
    print_balances(&rpc, &setup)?;
    println!("run again with --resume after one hour to collect the second period");
    Ok(())
}
