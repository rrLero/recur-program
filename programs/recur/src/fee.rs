use anchor_lang::prelude::*;

use crate::{Config, Merchant, RecurError, MAX_BPS};

pub fn bps_of(amount: u64, bps: u16) -> Result<u64> {
    let share = (amount as u128)
        .checked_mul(bps as u128)
        .ok_or(RecurError::MathOverflow)?
        / MAX_BPS as u128;
    u64::try_from(share).map_err(|_| RecurError::MathOverflow.into())
}

pub fn fee_for(amount: u64, fee_bps: u16, min_fee: u64) -> Result<u64> {
    Ok(bps_of(amount, fee_bps)?.max(min_fee).min(amount))
}

pub fn effective_fee_bps(config: &Config, merchant: &Merchant) -> u16 {
    merchant.fee_bps_override.unwrap_or(config.fee_bps)
}

pub struct Shares {
    pub fee: u64,
    pub referral: u64,
    pub merchant: u64,
}

pub fn split(
    amount: u64,
    config: &Config,
    merchant: &Merchant,
    referral_bps: u16,
    now: i64,
) -> Result<Shares> {
    let fee = if now < merchant.fee_free_until {
        0
    } else {
        fee_for(amount, effective_fee_bps(config, merchant), config.min_fee)?
    };
    let after_fee = amount.checked_sub(fee).ok_or(RecurError::MathOverflow)?;
    let referral = bps_of(after_fee, referral_bps)?;
    let merchant = after_fee
        .checked_sub(referral)
        .ok_or(RecurError::MathOverflow)?;
    Ok(Shares {
        fee,
        referral,
        merchant,
    })
}

pub fn per_call_split(
    amount: u64,
    config: &Config,
    merchant: &Merchant,
    now: i64,
) -> Result<Shares> {
    let fee = if now < merchant.fee_free_until {
        0
    } else {
        bps_of(amount, effective_fee_bps(config, merchant))?
    };
    let merchant = amount.checked_sub(fee).ok_or(RecurError::MathOverflow)?;
    Ok(Shares {
        fee,
        referral: 0,
        merchant,
    })
}
