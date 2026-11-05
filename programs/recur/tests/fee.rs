use anchor_lang::prelude::Pubkey;
use recur::{split, Config, Merchant};

const USDC: u64 = 1_000_000;

fn config() -> Config {
    Config {
        version: 2,
        bump: 255,
        admin: Pubkey::default(),
        fee_bps: 200,
        min_fee: 100_000,
        treasury: Pubkey::default(),
        paused: false,
        agent_puller: Pubkey::default(),
        reserved: [0; 32],
    }
}

fn merchant(fee_bps_override: Option<u16>, fee_free_until: i64) -> Merchant {
    Merchant {
        version: 1,
        bump: 255,
        owner: Pubkey::default(),
        referral_bps: 0,
        fee_bps_override,
        fee_free_until,
        reserved: [0; 64],
    }
}

#[test]
fn split_takes_fee_then_referral_from_the_rest() {
    let s = split(30 * USDC, &config(), &merchant(None, 0), 1_000, 10).unwrap();
    assert_eq!(
        (s.fee, s.referral, s.merchant),
        (600_000, 2_940_000, 26_460_000)
    );
}

#[test]
fn split_is_fee_free_inside_window_even_below_minimum() {
    let s = split(USDC, &config(), &merchant(None, 11), 0, 10).unwrap();
    assert_eq!((s.fee, s.merchant), (0, USDC));
}

#[test]
fn split_charges_again_when_window_ends() {
    let s = split(USDC, &config(), &merchant(None, 10), 0, 10).unwrap();
    assert_eq!(s.fee, 100_000);
}

#[test]
fn split_uses_override() {
    let s = split(30 * USDC, &config(), &merchant(Some(100), 0), 0, 10).unwrap();
    assert_eq!(s.fee, 300_000);
}

#[test]
fn split_parts_sum_to_amount() {
    for amount in [1, 99, 100_000, 30 * USDC, u64::MAX / 10_000] {
        for referral_bps in [0, 1_000, 10_000] {
            let s = split(amount, &config(), &merchant(None, 0), referral_bps, 10).unwrap();
            assert_eq!(s.fee + s.referral + s.merchant, amount);
        }
    }
}
