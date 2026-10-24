pub mod errors;
pub mod fee;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use errors::*;
pub use fee::*;
pub use instructions::*;
pub use state::*;

declare_id!("bY4Yyya6CMHBUfU9BfgoWK8TCjVhWtxQPurtX56dF5v");

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

    pub fn collect(ctx: Context<Collect>, amount: u64, referral_bps: u16) -> Result<()> {
        instructions::collect::handle_collect(ctx, amount, referral_bps)
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
}
