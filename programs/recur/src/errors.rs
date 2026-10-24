use anchor_lang::prelude::*;

#[error_code]
pub enum RecurError {
    #[msg("Fee basis points can't exceed 10000")]
    FeeTooHigh,
    #[msg("Recur is paused")]
    Paused,
    #[msg("The plan doesn't whitelist the Recur puller")]
    NotRecurPuller,
    #[msg("Arithmetic overflow")]
    MathOverflow,
}
