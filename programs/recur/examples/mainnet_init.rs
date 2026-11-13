#[path = "../tests/common/ix.rs"]
mod ix;

use std::{env, str::FromStr};

use anchor_lang::AccountDeserialize;
use recur::Config;
use solana_keypair::read_keypair_file;
use solana_pubkey::Pubkey;
use solana_rpc_client::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_transaction::Transaction;

const FEE_BPS: u16 = 200;
const MIN_FEE: u64 = 100_000;
const USAGE: &str = "usage: mainnet_init <deployer keypair> <rpc url> <squads multisig> \
<squads vault> <usdc mint> <agent puller> [--send]";

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn address(arg: Option<String>) -> Res<Pubkey> {
    Ok(Pubkey::from_str(&arg.ok_or(USAGE)?)?)
}

fn main() -> Res<()> {
    let mut args = env::args().skip(1);
    let deployer = read_keypair_file(args.next().ok_or(USAGE)?)?;
    let rpc = RpcClient::new(args.next().ok_or(USAGE)?);
    let multisig = address(args.next())?;
    let vault = address(args.next())?;
    let mint = address(args.next())?;
    let agent_puller = address(args.next())?;
    let send = args.next().as_deref() == Some("--send");

    if rpc.get_account(&multisig)?.owner != ix::SQUADS_PROGRAM {
        return Err("the multisig account isn't owned by the Squads program".into());
    }
    if ix::squads_vault_pda(&multisig) != vault {
        return Err("the vault isn't vault 0 of that multisig".into());
    }
    if rpc.get_account(&mint)?.owner != anchor_spl::token::ID {
        return Err("the mint isn't a Token program mint".into());
    }
    let config_address = ix::config_pda();
    if rpc
        .get_account_with_commitment(&config_address, rpc.commitment())?
        .value
        .is_some()
    {
        return Err("the config already exists".into());
    }

    let ixs = ix::handover_ixs(&ix::Handover {
        deployer: deployer.pubkey(),
        vault,
        mint,
        agent_puller,
        fee_bps: FEE_BPS,
        min_fee: MIN_FEE,
    });
    let tx = Transaction::new_signed_with_payer(
        &ixs,
        Some(&deployer.pubkey()),
        &[&deployer],
        rpc.get_latest_blockhash()?,
    );

    println!("program:      {}", recur::ID);
    println!("config:       {config_address}");
    println!("admin, treasury (vault): {vault}");
    println!("treasury token account:  {}", ix::ata_of(&mint, &vault));
    println!("agent puller: {agent_puller}");
    println!("fee: {FEE_BPS} bps, minimum {MIN_FEE}; paused");

    let simulation = rpc.simulate_transaction(&tx)?.value;
    for line in simulation.logs.unwrap_or_default() {
        println!("  {line}");
    }
    if let Some(err) = simulation.err {
        return Err(format!("simulation failed: {err:?}").into());
    }
    if !send {
        println!("simulation passed; run again with --send to submit");
        return Ok(());
    }

    println!("sent: {}", rpc.send_and_confirm_transaction(&tx)?);
    let data = rpc.get_account(&config_address)?.data;
    let config = Config::try_deserialize(&mut data.as_slice())?;
    assert_eq!(
        (
            config.admin,
            config.treasury,
            config.agent_puller,
            config.paused
        ),
        (vault, vault, agent_puller, true),
        "the config doesn't match what was sent"
    );
    println!("config.admin is the vault; the program is paused");
    Ok(())
}
