pub mod errors;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use errors::*;
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
        instructions::init_config::handle(ctx, fee_bps, min_fee, treasury)
    }
}
