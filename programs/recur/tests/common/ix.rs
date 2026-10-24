#![allow(dead_code)]

use anchor_lang::{InstructionData, ToAccountMetas};
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

pub fn config_pda() -> Pubkey {
    Pubkey::find_program_address(&[recur::CONFIG_SEED], &recur::ID).0
}

pub fn puller_pda() -> Pubkey {
    Pubkey::find_program_address(&[recur::PULLER_SEED], &recur::ID).0
}

pub fn transit_pda() -> Pubkey {
    Pubkey::find_program_address(&[recur::TRANSIT_SEED], &recur::ID).0
}

pub fn program_data_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[recur::ID.as_ref()],
        &solana_sdk_ids::bpf_loader_upgradeable::ID,
    )
    .0
}

pub fn init_config_ix(admin: Pubkey, fee_bps: u16, min_fee: u64, treasury: Pubkey) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: recur::accounts::InitConfig {
            admin,
            config: config_pda(),
            program: recur::ID,
            program_data: program_data_pda(),
            system_program: solana_sdk_ids::system_program::ID,
        }
        .to_account_metas(None),
        data: recur::instruction::InitConfig {
            fee_bps,
            min_fee,
            treasury,
        }
        .data(),
    }
}

pub fn update_config_ix(
    signer: Pubkey,
    admin: Pubkey,
    fee_bps: u16,
    min_fee: u64,
    treasury: Pubkey,
    paused: bool,
) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: recur::accounts::UpdateConfig {
            admin: signer,
            config: config_pda(),
        }
        .to_account_metas(None),
        data: recur::instruction::UpdateConfig {
            admin,
            fee_bps,
            min_fee,
            treasury,
            paused,
        }
        .data(),
    }
}

pub fn sweep_transit_ix(admin: Pubkey, transit_ata: Pubkey, treasury_ata: Pubkey) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: recur::accounts::SweepTransit {
            admin,
            config: config_pda(),
            transit: transit_pda(),
            transit_ata,
            treasury_ata,
            token_program: litesvm_token::spl_token::ID,
        }
        .to_account_metas(None),
        data: recur::instruction::SweepTransit {}.data(),
    }
}

pub fn collect_ix(
    accounts: recur::accounts::Collect,
    amount: u64,
    referral_bps: u16,
) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: accounts.to_account_metas(None),
        data: recur::instruction::Collect {
            amount,
            referral_bps,
        }
        .data(),
    }
}
