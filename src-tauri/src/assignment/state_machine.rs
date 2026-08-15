use std::time::Duration;

use crate::{
    domain::assignment::{AssignmentSideEffect, AssignmentStatus},
    error::AppError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentAction {
    Claim,
    Start,
    Complete,
    Fail,
    Wait,
    Cancel,
    Interrupt,
    Retry,
    DeadLetter,
    RequireRecoveryConfirmation,
    ConfirmResume,
    Resume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryDecision {
    Requeue,
    RequireConfirmation,
}

pub fn transition(
    current: AssignmentStatus,
    action: AssignmentAction,
) -> Result<AssignmentStatus, AppError> {
    match (current, action) {
        (AssignmentStatus::Queued, AssignmentAction::Claim) => Ok(AssignmentStatus::Claimed),
        (AssignmentStatus::Claimed, AssignmentAction::Start) => Ok(AssignmentStatus::Running),
        (AssignmentStatus::Running, AssignmentAction::Complete) => Ok(AssignmentStatus::Completed),
        (AssignmentStatus::Running, AssignmentAction::Fail) => Ok(AssignmentStatus::Failed),
        (AssignmentStatus::Running, AssignmentAction::Wait) => Ok(AssignmentStatus::Waiting),
        (AssignmentStatus::Running, AssignmentAction::Cancel) => Ok(AssignmentStatus::Cancelled),
        (AssignmentStatus::Running, AssignmentAction::Interrupt) => {
            Ok(AssignmentStatus::Interrupted)
        }
        (AssignmentStatus::Failed | AssignmentStatus::Interrupted, AssignmentAction::Retry) => {
            Ok(AssignmentStatus::Queued)
        }
        (
            AssignmentStatus::Failed | AssignmentStatus::Interrupted,
            AssignmentAction::DeadLetter,
        ) => Ok(AssignmentStatus::DeadLetter),
        (AssignmentStatus::Interrupted, AssignmentAction::RequireRecoveryConfirmation) => {
            Ok(AssignmentStatus::RecoveryConfirmationRequired)
        }
        (AssignmentStatus::RecoveryConfirmationRequired, AssignmentAction::ConfirmResume)
        | (AssignmentStatus::Waiting, AssignmentAction::Resume) => Ok(AssignmentStatus::Queued),
        (
            AssignmentStatus::RecoveryConfirmationRequired | AssignmentStatus::Waiting,
            AssignmentAction::Cancel,
        ) => Ok(AssignmentStatus::Cancelled),
        _ => Err(AppError::invalid_input(
            "assignmentStatus",
            "assignment status transition is invalid",
        )),
    }
}

pub fn recovery_decision(
    side_effect: AssignmentSideEffect,
    attempt_started: bool,
) -> RecoveryDecision {
    if attempt_started
        && matches!(
            side_effect,
            AssignmentSideEffect::NonIdempotentWrite | AssignmentSideEffect::Unknown
        )
    {
        RecoveryDecision::RequireConfirmation
    } else {
        RecoveryDecision::Requeue
    }
}

pub fn retry_delay(attempt: u32, base: Duration, max: Duration, jitter_seed: u64) -> Duration {
    if attempt == 0 || base.is_zero() || max.is_zero() {
        return Duration::ZERO;
    }
    if base >= max || attempt > 32 {
        return max;
    }

    let multiplier = 1_u32 << (attempt - 1);
    let Some(exponential) = base.checked_mul(multiplier) else {
        return max;
    };
    if exponential >= max {
        return max;
    }

    // SplitMix64 gives stable, injected jitter without global randomness.
    let mut value = jitter_seed ^ u64::from(attempt);
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;

    let nanos = exponential.as_nanos();
    let jittered = nanos / 2 + nanos.saturating_mul(u128::from(value)) / u128::from(u64::MAX);
    duration_from_nanos(jittered.min(max.as_nanos()))
}

fn duration_from_nanos(nanos: u128) -> Duration {
    let seconds = nanos / 1_000_000_000;
    if seconds > u128::from(u64::MAX) {
        return Duration::MAX;
    }
    Duration::new(seconds as u64, (nanos % 1_000_000_000) as u32)
}
