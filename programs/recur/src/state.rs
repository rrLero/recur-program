use anchor_lang::prelude::*;

pub const CONFIG_SEED: &[u8] = b"config";
pub const CONFIG_VERSION: u8 = 1;
pub const MAX_BPS: u16 = 10_000;

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub version: u8,
    pub bump: u8,
    pub admin: Pubkey,
    pub fee_bps: u16,
    pub min_fee: u64,
    pub treasury: Pubkey,
    pub paused: bool,
    pub reserved: [u8; 64],
}

pub const PULLER_SEED: &[u8] = b"puller";
pub const TRANSIT_SEED: &[u8] = b"transit";
