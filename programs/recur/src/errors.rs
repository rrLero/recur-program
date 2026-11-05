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
    #[msg("Collect must pull exactly the plan's price for one period")]
    AmountMismatch,
    #[msg("Only the program's upgrade authority can do this")]
    NotUpgradeAuthority,
    #[msg("Only the admin can do this")]
    Unauthorized,
    #[msg("Referral basis points can't exceed 10000")]
    ReferralTooHigh,
    #[msg("A member can't refer themselves")]
    SelfReferral,
    #[msg("The member already has an active membership")]
    AlreadyMember,
    #[msg("The referrer account doesn't match the membership")]
    ReferrerMismatch,
    #[msg("The rail plan isn't active")]
    PlanNotActive,
    #[msg("The membership isn't a comp")]
    NotComp,
    #[msg("The comp end must be in the future")]
    InvalidCompEnd,
    #[msg("The membership can't be extended this way")]
    CannotExtend,
    #[msg("The membership is still active")]
    MembershipActive,
    #[msg("The rail subscription is still live")]
    SubscriptionLive,
    #[msg("Upfront payments need a plan Recur doesn't pull from")]
    NotUpfrontPlan,
    #[msg("Membership is prepaid or comped until paid_until")]
    PrepaidAccessActive,
    #[msg("The agent puller isn't set")]
    AgentPullerNotSet,
    #[msg("The plan doesn't pay only into the merchant's transit")]
    NotAgentPlan,
    #[msg("The rail subscription hasn't pulled this period's price")]
    PeriodNotPulled,
    #[msg("This period is already settled")]
    PeriodAlreadySettled,
    #[msg("The subscription period has already ended")]
    PeriodEnded,
    #[msg("The transit doesn't hold that amount")]
    InsufficientTransit,
    #[msg("The pull didn't raise the transit by the period's price")]
    TransitShortfall,
}
