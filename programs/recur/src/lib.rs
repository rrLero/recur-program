pub mod errors;
pub mod fee;
pub mod instructions;
pub mod rail;
pub mod state;

use anchor_lang::prelude::*;

pub use errors::*;
pub use fee::*;
pub use instructions::*;
pub use state::*;

declare_id!("bY4Yyya6CMHBUfU9BfgoWK8TCjVhWtxQPurtX56dF5v");

#[cfg(not(feature = "no-entrypoint"))]
solana_security_txt::security_txt! {
    name: "Recur",
    project_url: "https://www.recurme.com",
    contacts: "email:security@recurme.com,link:https://github.com/rrLero/recur-program/security/advisories/new",
    policy: "https://github.com/rrLero/recur-program/blob/main/SECURITY.md",
    preferred_languages: "en",
    source_code: "https://github.com/rrLero/recur-program"
}

#[program]
pub mod recur {
    use super::*;

    pub fn init_config(
        ctx: Context<InitConfig>,
        fee_bps: u16,
        min_fee: u64,
        treasury: Pubkey,
    ) -> Result<()> {
        instructions::init_config::handle_init_config(ctx, fee_bps, min_fee, treasury)
    }

    pub fn collect(ctx: Context<Collect>) -> Result<()> {
        instructions::collect::handle_collect(ctx)
    }

    pub fn subscribe_hook(ctx: Context<SubscribeHook>, referrer: Option<Pubkey>) -> Result<()> {
        instructions::subscribe_hook::handle_subscribe_hook(ctx, referrer)
    }

    pub fn pay_upfront(ctx: Context<PayUpfront>, referrer: Option<Pubkey>) -> Result<()> {
        instructions::pay_upfront::handle_pay_upfront(ctx, referrer)
    }

    pub fn update_config(
        ctx: Context<UpdateConfig>,
        new_admin: Pubkey,
        fee_bps: u16,
        min_fee: u64,
        treasury: Pubkey,
        paused: bool,
    ) -> Result<()> {
        instructions::update_config::handle_update_config(
            ctx, new_admin, fee_bps, min_fee, treasury, paused,
        )
    }

    pub fn sweep_transit(ctx: Context<SweepTransit>) -> Result<()> {
        instructions::sweep_transit::handle_sweep_transit(ctx)
    }

    pub fn register_merchant(ctx: Context<RegisterMerchant>, referral_bps: u16) -> Result<()> {
        instructions::register_merchant::handle_register_merchant(ctx, referral_bps)
    }

    pub fn update_merchant(ctx: Context<UpdateMerchant>, referral_bps: u16) -> Result<()> {
        instructions::register_merchant::handle_update_merchant(ctx, referral_bps)
    }

    pub fn set_merchant_terms(
        ctx: Context<SetMerchantTerms>,
        fee_bps_override: Option<u16>,
        fee_free_until: i64,
    ) -> Result<()> {
        instructions::set_merchant_terms::handle_set_merchant_terms(
            ctx,
            fee_bps_override,
            fee_free_until,
        )
    }

    pub fn grant_comp(ctx: Context<GrantComp>, member: Pubkey, until: i64) -> Result<()> {
        instructions::comp::handle_grant_comp(ctx, member, until)
    }

    pub fn revoke_comp(ctx: Context<RevokeComp>) -> Result<()> {
        instructions::comp::handle_revoke_comp(ctx)
    }

    pub fn refund(ctx: Context<Refund>, amount: u64, paid_until: Option<i64>) -> Result<()> {
        instructions::refund::handle_refund(ctx, amount, paid_until)
    }

    pub fn close(ctx: Context<Close>) -> Result<()> {
        instructions::close::handle_close(ctx)
    }
}
