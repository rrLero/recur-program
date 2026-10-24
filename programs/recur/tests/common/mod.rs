#![allow(dead_code)]

use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use litesvm::{types::FailedTransactionMetadata, LiteSVM};
use recur::{Config, RecurError};
use solana_instruction::{error::InstructionError, Instruction};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::Transaction;
use solana_transaction_error::TransactionError;

pub type TxResult = Result<(), Box<FailedTransactionMetadata>>;

pub struct Ctx {
    pub svm: LiteSVM,
    pub admin: Keypair,
}

pub fn setup() -> Ctx {
    let mut svm = LiteSVM::new();
    let so = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/recur.so");
    svm.add_program_from_file(recur::ID, so)
        .expect("run `anchor build` first");
    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    Ctx { svm, admin }
}

pub fn new_address() -> Pubkey {
    Pubkey::new_unique()
}

pub fn config_pda() -> Pubkey {
    Pubkey::find_program_address(&[recur::CONFIG_SEED], &recur::ID).0
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
    let ix = Instruction {
        program_id: recur::ID,
        accounts: recur::accounts::InitConfig {
            admin: ctx.admin.pubkey(),
            config: config_pda(),
            system_program: solana_sdk_ids::system_program::ID,
        }
        .to_account_metas(None),
        data: recur::instruction::InitConfig {
            fee_bps,
            min_fee,
            treasury,
        }
        .data(),
    };
    let admin = ctx.admin.insecure_clone();
    send(ctx, ix, &[&admin])
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
