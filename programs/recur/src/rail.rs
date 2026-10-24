use anchor_lang::prelude::*;
use subscriptions::{
    types::{AccountDiscriminator, PlanTerms},
    Plan, SubscriptionDelegation, SUBSCRIPTIONS_ID,
};

use crate::RecurError;

pub fn read_plan(account: &UncheckedAccount) -> Result<Plan> {
    let plan = Plan::from_bytes(&account.try_borrow_data()?)
        .map_err(|_| error!(ErrorCode::AccountDidNotDeserialize))?;
    require_eq!(
        plan.discriminator,
        AccountDiscriminator::Plan as u8,
        ErrorCode::AccountDiscriminatorMismatch
    );
    Ok(plan)
}

pub fn read_subscription(account: &UncheckedAccount) -> Result<SubscriptionDelegation> {
    decode_subscription(account)
}

fn decode_subscription(account: &AccountInfo) -> Result<SubscriptionDelegation> {
    let subscription = SubscriptionDelegation::from_bytes(&account.try_borrow_data()?)
        .map_err(|_| error!(ErrorCode::AccountDidNotDeserialize))?;
    require_eq!(
        subscription.header.discriminator,
        AccountDiscriminator::SubscriptionDelegation as u8,
        ErrorCode::AccountDiscriminatorMismatch
    );
    Ok(subscription)
}

pub fn subscription_is_over(account: &AccountInfo, now: i64) -> bool {
    if account.lamports() == 0 || account.owner != &SUBSCRIPTIONS_ID {
        return true;
    }
    match decode_subscription(account) {
        Ok(subscription) => subscription.expires_at_ts != 0 && now >= subscription.expires_at_ts,
        Err(_) => false,
    }
}

pub fn period_secs(terms: &PlanTerms) -> Result<i64> {
    i64::try_from(terms.period_hours)
        .ok()
        .and_then(|hours| hours.checked_mul(3600))
        .ok_or_else(|| error!(RecurError::MathOverflow))
}
