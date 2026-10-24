use anchor_lang::prelude::*;

use crate::RecurError;

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

pub const MERCHANT_SEED: &[u8] = b"merchant";
pub const MERCHANT_VERSION: u8 = 1;

#[account]
#[derive(InitSpace)]
pub struct Merchant {
    pub version: u8,
    pub bump: u8,
    pub owner: Pubkey,
    pub referral_bps: u16,
    pub fee_bps_override: Option<u16>,
    pub fee_free_until: i64,
    pub reserved: [u8; 64],
}

pub const PULLER_SEED: &[u8] = b"puller";
pub const TRANSIT_SEED: &[u8] = b"transit";

pub const MEMBERSHIP_SEED: &[u8] = b"membership";
pub const MEMBERSHIP_VERSION: u8 = 1;

#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MembershipSource {
    Paid,
    Comp,
    Upfront,
}

#[account]
#[derive(InitSpace)]
pub struct Membership {
    pub version: u8,
    pub bump: u8,
    pub member: Pubkey,
    pub plan: Pubkey,
    pub merchant: Pubkey,
    pub paid_until: i64,
    pub source: MembershipSource,
    pub referrer: Option<Pubkey>,
    pub referral_bps: u16,
    pub rent_payer: Pubkey,
    pub created_at: i64,
    pub reserved: [u8; 64],
}

pub struct MembershipIdentity {
    pub bump: u8,
    pub member: Pubkey,
    pub plan: Pubkey,
    pub merchant: Pubkey,
    pub rent_payer: Pubkey,
}

impl Membership {
    pub fn can_reopen(&self, now: i64) -> bool {
        self.created_at == 0 || self.paid_until <= now || self.source == MembershipSource::Comp
    }

    pub fn open(
        &mut self,
        identity: MembershipIdentity,
        referrer: Option<Pubkey>,
        referral_bps: u16,
        now: i64,
    ) -> Result<()> {
        if let Some(referrer) = referrer {
            require!(
                !is_self_referral(&referrer, &identity.member),
                RecurError::SelfReferral
            );
        }
        if self.created_at == 0 {
            self.version = MEMBERSHIP_VERSION;
            self.bump = identity.bump;
            self.member = identity.member;
            self.plan = identity.plan;
            self.merchant = identity.merchant;
            self.rent_payer = identity.rent_payer;
            self.created_at = now;
        }
        self.referrer = referrer;
        self.referral_bps = referral_bps;
        Ok(())
    }
}

fn is_self_referral(referrer: &Pubkey, member: &Pubkey) -> bool {
    referrer == member
        || [CONFIG_SEED, PULLER_SEED, TRANSIT_SEED]
            .iter()
            .any(|seed| *referrer == Pubkey::find_program_address(&[seed], &crate::ID).0)
}
