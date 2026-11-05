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

pub fn merchant_transit_pda(owner: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[recur::TRANSIT_SEED, owner.as_ref()], &recur::ID).0
}

pub fn merchant_pda(owner: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[recur::MERCHANT_SEED, owner.as_ref()], &recur::ID).0
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
    new_admin: Pubkey,
    fee_bps: u16,
    min_fee: u64,
    treasury: Pubkey,
    paused: bool,
    agent_puller: Pubkey,
) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: recur::accounts::UpdateConfig {
            admin: signer,
            config: config_pda(),
        }
        .to_account_metas(None),
        data: recur::instruction::UpdateConfig {
            new_admin,
            fee_bps,
            min_fee,
            treasury,
            paused,
            agent_puller,
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

pub fn membership_pda(plan: &Pubkey, member: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[recur::MEMBERSHIP_SEED, plan.as_ref(), member.as_ref()],
        &recur::ID,
    )
    .0
}

pub fn rail_subscription_pda(plan: &Pubkey, member: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"subscription", plan.as_ref(), member.as_ref()],
        &subscriptions::SUBSCRIPTIONS_ID,
    )
    .0
}

pub fn collect_ix(accounts: recur::accounts::Collect) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: accounts.to_account_metas(None),
        data: recur::instruction::Collect {}.data(),
    }
}

pub fn activate_agent_ix(accounts: recur::accounts::ActivateAgent) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: accounts.to_account_metas(None),
        data: recur::instruction::ActivateAgent {}.data(),
    }
}

pub fn split_ix(accounts: recur::accounts::Split, amount: u64) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: accounts.to_account_metas(None),
        data: recur::instruction::Split { amount }.data(),
    }
}

pub fn subscribe_hook_ix(
    member: Pubkey,
    payer: Pubkey,
    plan: Pubkey,
    merchant_owner: Pubkey,
    referrer: Option<Pubkey>,
) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: recur::accounts::SubscribeHook {
            member,
            payer,
            config: config_pda(),
            plan,
            merchant: merchant_pda(&merchant_owner),
            membership: membership_pda(&plan, &member),
            subscription: rail_subscription_pda(&plan, &member),
            system_program: solana_sdk_ids::system_program::ID,
        }
        .to_account_metas(None),
        data: recur::instruction::SubscribeHook { referrer }.data(),
    }
}

pub fn register_merchant_ix(owner: Pubkey, payer: Pubkey, referral_bps: u16) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: recur::accounts::RegisterMerchant {
            owner,
            payer,
            merchant: merchant_pda(&owner),
            system_program: solana_sdk_ids::system_program::ID,
        }
        .to_account_metas(None),
        data: recur::instruction::RegisterMerchant { referral_bps }.data(),
    }
}

pub fn update_merchant_ix(
    signer: Pubkey,
    merchant_owner: Pubkey,
    referral_bps: u16,
) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: recur::accounts::UpdateMerchant {
            owner: signer,
            merchant: merchant_pda(&merchant_owner),
        }
        .to_account_metas(None),
        data: recur::instruction::UpdateMerchant { referral_bps }.data(),
    }
}

pub fn set_merchant_terms_ix(
    admin: Pubkey,
    merchant_owner: Pubkey,
    fee_bps_override: Option<u16>,
    fee_free_until: i64,
) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: recur::accounts::SetMerchantTerms {
            admin,
            config: config_pda(),
            merchant: merchant_pda(&merchant_owner),
        }
        .to_account_metas(None),
        data: recur::instruction::SetMerchantTerms {
            fee_bps_override,
            fee_free_until,
        }
        .data(),
    }
}

pub fn pay_upfront_ix(
    accounts: recur::accounts::PayUpfront,
    referrer: Option<Pubkey>,
) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: accounts.to_account_metas(None),
        data: recur::instruction::PayUpfront { referrer }.data(),
    }
}

pub fn grant_comp_ix(
    owner: Pubkey,
    payer: Pubkey,
    plan: Pubkey,
    member: Pubkey,
    until: i64,
) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: recur::accounts::GrantComp {
            owner,
            payer,
            plan,
            membership: membership_pda(&plan, &member),
            system_program: solana_sdk_ids::system_program::ID,
        }
        .to_account_metas(None),
        data: recur::instruction::GrantComp { member, until }.data(),
    }
}

pub fn revoke_comp_ix(owner: Pubkey, plan: Pubkey, member: Pubkey) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: recur::accounts::RevokeComp {
            owner,
            plan,
            membership: membership_pda(&plan, &member),
        }
        .to_account_metas(None),
        data: recur::instruction::RevokeComp {}.data(),
    }
}

pub fn refund_ix(
    accounts: recur::accounts::Refund,
    amount: u64,
    paid_until: Option<i64>,
) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: accounts.to_account_metas(None),
        data: recur::instruction::Refund { amount, paid_until }.data(),
    }
}

pub fn close_ix(accounts: recur::accounts::Close) -> Instruction {
    Instruction {
        program_id: recur::ID,
        accounts: accounts.to_account_metas(None),
        data: recur::instruction::Close {}.data(),
    }
}
