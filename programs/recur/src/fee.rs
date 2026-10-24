use anchor_lang::prelude::*;

use crate::{RecurError, MAX_BPS};

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
