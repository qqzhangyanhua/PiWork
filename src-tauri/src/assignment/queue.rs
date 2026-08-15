//! Per-Work Assignment queue state machine.
//!
//! Upstream path: `crates/buzz-acp/src/queue.rs` in `block/buzz`
//! Commit: `5bf78671f45178f8de02ba18d3d321cbbf19cd1f`
//! PiWork differences: each claim owns exactly one Work/Assignment; Buzz event
//! batches become bounded, ordered input references within that Assignment.
//! Global/per-Agent capacity, ownership-aware depth, observable expiry, and
//! fail-closed validation are added. Relay, Nostr, ACP prompt formatting,
//! drop-mode dedup, retry jitter, and native-steer transport are omitted.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use chrono::{DateTime, TimeDelta, Utc};
use thiserror::Error;

pub const MAX_IN_FLIGHT_TIMEOUT_SECONDS: i64 = 86_400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueInput {
    pub id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub id: String,
    pub work_id: String,
    pub agent_id: String,
    pub created_at: DateTime<Utc>,
    pub not_before: DateTime<Utc>,
    pub retry_count: u32,
    pub inputs: Vec<QueueInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueLimits {
    pub max_pending_per_work: usize,
    pub max_batch_size: usize,
    pub global_parallelism: usize,
    pub default_agent_parallelism: usize,
    pub agent_parallelism: BTreeMap<String, usize>,
    pub in_flight_timeout: TimeDelta,
    pub max_retries: u32,
    pub retry_base: TimeDelta,
    pub retry_max: TimeDelta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueClaim {
    pub id: String,
    pub work_id: String,
    pub agent_id: String,
    pub item: QueueItem,
    pub claimed_at: DateTime<Utc>,
    pub deadline: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueOutcome {
    Completed,
    RetryableFailure,
    PoolExhausted,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueCompletion {
    pub claim_id: String,
    pub completed_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub outcome: QueueOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueRelease {
    Completed {
        item: QueueItem,
        continuation_inputs: Vec<QueueInput>,
    },
    Requeued {
        item: QueueItem,
        available_at: DateTime<Utc>,
        retry_count: u32,
    },
    DeadLettered {
        item: QueueItem,
        retry_count: u32,
    },
    Cancelled {
        item: QueueItem,
    },
    Expired {
        item: QueueItem,
        deadline: DateTime<Utc>,
    },
    Rejected {
        claim_id: String,
        reason: CompletionRejection,
        item: Option<QueueItem>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionRejection {
    UnknownOrExpiredClaim,
    CompletionBeforeClaim,
    InvalidCompletionTimes,
    TimeOverflow,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum QueueError {
    #[error("queue limits are invalid: {0}")]
    InvalidLimits(&'static str),
    #[error("queue item has an invalid {field}")]
    InvalidItem { field: &'static str },
    #[error("queue item id already exists: {0}")]
    DuplicateId(String),
    #[error("owned queue depth for Work {work_id} reached its cap of {limit}")]
    DepthExceeded { work_id: String, limit: usize },
    #[error("assignment queue item not found: {0}")]
    UnknownAssignment(String),
    #[error("input batch for Assignment {assignment_id} reached its cap of {limit}")]
    BatchExceeded { assignment_id: String, limit: usize },
    #[error("queue input has an invalid {field}")]
    InvalidInput { field: &'static str },
}

/// Pure in-memory scheduling state derived from Buzz's per-Channel queue.
pub struct WorkQueue {
    per_work: BTreeMap<String, VecDeque<QueueItem>>,
    inflight_by_work: BTreeMap<String, QueueClaim>,
    inflight_by_agent: BTreeMap<String, usize>,
    limits: QueueLimits,
    withheld_inputs: BTreeMap<String, Vec<QueueInput>>,
    known_ids: BTreeSet<String>,
    next_claim_sequence: u64,
}

impl WorkQueue {
    pub fn hydrate(
        items: impl IntoIterator<Item = QueueItem>,
        limits: QueueLimits,
    ) -> Result<Self, QueueError> {
        validate_limits(&limits)?;
        let mut queue = Self {
            per_work: BTreeMap::new(),
            inflight_by_work: BTreeMap::new(),
            inflight_by_agent: BTreeMap::new(),
            limits,
            withheld_inputs: BTreeMap::new(),
            known_ids: BTreeSet::new(),
            next_claim_sequence: 0,
        };
        for item in items {
            queue.push(item)?;
        }
        Ok(queue)
    }

    pub fn push(&mut self, mut item: QueueItem) -> Result<(), QueueError> {
        validate_item(&mut item, &self.limits)?;
        if self.known_ids.contains(&item.id) {
            return Err(QueueError::DuplicateId(item.id));
        }
        self.ensure_ownership_capacity(&item.work_id, 1)?;

        self.known_ids.insert(item.id.clone());
        insert_sorted(self.per_work.entry(item.work_id.clone()).or_default(), item);
        Ok(())
    }

    pub fn push_input(&mut self, assignment_id: &str, input: QueueInput) -> Result<(), QueueError> {
        validate_input(&input)?;
        let pending_location = self.per_work.iter().find_map(|(work_id, items)| {
            items
                .iter()
                .position(|item| item.id == assignment_id)
                .map(|position| (work_id.clone(), position))
        });
        if let Some((work_id, position)) = pending_location {
            let item = self
                .per_work
                .get_mut(&work_id)
                .and_then(|items| items.get_mut(position))
                .expect("pending Assignment location remains valid");
            ensure_input_capacity_and_uniqueness(
                assignment_id,
                &item.inputs,
                &[],
                &input,
                self.limits.max_batch_size,
            )?;
            insert_input_sorted(&mut item.inputs, input);
            return Ok(());
        }

        let claim = self
            .inflight_by_work
            .values()
            .find(|claim| claim.item.id == assignment_id)
            .ok_or_else(|| QueueError::UnknownAssignment(assignment_id.to_owned()))?;
        let withheld = self
            .withheld_inputs
            .get(assignment_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        ensure_input_capacity_and_uniqueness(
            assignment_id,
            &claim.item.inputs,
            withheld,
            &input,
            self.limits.max_batch_size,
        )?;
        insert_input_sorted(
            self.withheld_inputs
                .entry(assignment_id.to_owned())
                .or_default(),
            input,
        );
        Ok(())
    }

    pub fn expire_deadlines(&mut self, now: DateTime<Utc>) -> Vec<QueueRelease> {
        let expired_work_ids: Vec<String> = self
            .inflight_by_work
            .iter()
            .filter(|(_, claim)| claim.deadline <= now)
            .map(|(work_id, _)| work_id.clone())
            .collect();
        let mut expired_claims: Vec<QueueClaim> = expired_work_ids
            .into_iter()
            .filter_map(|work_id| self.take_claim(&work_id))
            .collect();
        expired_claims.sort_by(|left, right| compare_items(&left.item, &right.item));
        expired_claims
            .into_iter()
            .map(|claim| self.expired_release(claim))
            .collect()
    }

    pub fn next_claim(&mut self, now: DateTime<Utc>) -> Option<QueueClaim> {
        if self.inflight_by_work.len() >= self.limits.global_parallelism {
            return None;
        }

        let candidate = self
            .per_work
            .iter()
            .filter(|(work_id, _)| !self.inflight_by_work.contains_key(*work_id))
            .filter_map(|(work_id, items)| {
                let head = items.front()?;
                if head.not_before > now || !self.agent_has_capacity(&head.agent_id) {
                    return None;
                }
                Some((work_id.clone(), head.clone()))
            })
            .min_by(|(_, left), (_, right)| compare_items(left, right));
        let (work_id, head) = candidate?;
        let next_sequence = self.next_claim_sequence.checked_add(1)?;
        let deadline = now
            .checked_add_signed(self.limits.in_flight_timeout)
            .unwrap_or(DateTime::<Utc>::MAX_UTC);

        let item = self
            .per_work
            .get_mut(&work_id)
            .and_then(VecDeque::pop_front)
            .expect("candidate Work has a queue head");
        if self.per_work.get(&work_id).is_some_and(VecDeque::is_empty) {
            self.per_work.remove(&work_id);
        }

        let claim = QueueClaim {
            id: format!("claim:{next_sequence}:{}", head.id),
            work_id: work_id.clone(),
            agent_id: head.agent_id.clone(),
            item,
            claimed_at: now,
            deadline,
        };
        self.next_claim_sequence = next_sequence;
        *self
            .inflight_by_agent
            .entry(claim.agent_id.clone())
            .or_default() += 1;
        self.inflight_by_work.insert(work_id, claim.clone());
        Some(claim)
    }

    pub fn release(&mut self, completion: QueueCompletion) -> QueueRelease {
        let work_id = self
            .inflight_by_work
            .iter()
            .find(|(_, claim)| claim.id == completion.claim_id)
            .map(|(work_id, _)| work_id.clone());
        let Some(work_id) = work_id else {
            return rejected(
                completion.claim_id,
                CompletionRejection::UnknownOrExpiredClaim,
                None,
            );
        };
        let claim = self
            .inflight_by_work
            .get(&work_id)
            .expect("claim located by Work key");
        if completion.completed_at < claim.claimed_at || completion.observed_at < claim.claimed_at {
            return rejected(
                completion.claim_id,
                CompletionRejection::CompletionBeforeClaim,
                None,
            );
        }
        if completion.completed_at > completion.observed_at {
            return rejected(
                completion.claim_id,
                CompletionRejection::InvalidCompletionTimes,
                None,
            );
        }
        if completion.observed_at >= claim.deadline {
            let claim = self
                .take_claim(&work_id)
                .expect("late claim still exists after validation");
            return self.expired_release(claim);
        }

        let mut claim = self
            .take_claim(&work_id)
            .expect("claim still exists after validation");
        let withheld = self
            .withheld_inputs
            .remove(&claim.item.id)
            .unwrap_or_default();

        match completion.outcome {
            QueueOutcome::Completed => {
                let completed = claim.item;
                self.known_ids.remove(&completed.id);
                QueueRelease::Completed {
                    item: completed,
                    continuation_inputs: withheld,
                }
            }
            QueueOutcome::PoolExhausted => {
                merge_inputs(&mut claim.item.inputs, withheld);
                let item = claim.item;
                let available_at = item.not_before;
                let retry_count = item.retry_count;
                self.requeue_existing(item.clone());
                QueueRelease::Requeued {
                    item,
                    available_at,
                    retry_count,
                }
            }
            QueueOutcome::RetryableFailure => {
                merge_inputs(&mut claim.item.inputs, withheld);
                self.release_retry(claim.item, completion)
            }
            QueueOutcome::Cancelled => {
                merge_inputs(&mut claim.item.inputs, withheld);
                let item = claim.item;
                self.requeue_existing(item.clone());
                QueueRelease::Cancelled { item }
            }
        }
    }

    pub fn cancel_work(&mut self, work_id: &str) -> Vec<QueueItem> {
        let mut cancelled: Vec<QueueItem> = self
            .per_work
            .remove(work_id)
            .unwrap_or_default()
            .into_iter()
            .collect();
        if let Some(claim) = self.inflight_by_work.remove(work_id) {
            self.decrement_agent(&claim.agent_id);
            let mut item = claim.item;
            let withheld = self.withheld_inputs.remove(&item.id).unwrap_or_default();
            merge_inputs(&mut item.inputs, withheld);
            cancelled.push(item);
        }
        cancelled.sort_by(compare_items);
        for item in &cancelled {
            self.withheld_inputs.remove(&item.id);
            self.known_ids.remove(&item.id);
        }
        cancelled
    }

    fn agent_has_capacity(&self, agent_id: &str) -> bool {
        let limit = self
            .limits
            .agent_parallelism
            .get(agent_id)
            .copied()
            .unwrap_or(self.limits.default_agent_parallelism);
        self.inflight_by_agent.get(agent_id).copied().unwrap_or(0) < limit
    }

    fn owned_count(&self, work_id: &str) -> usize {
        self.per_work
            .get(work_id)
            .map_or(0, VecDeque::len)
            .saturating_add(usize::from(self.inflight_by_work.contains_key(work_id)))
    }

    fn ensure_ownership_capacity(&self, work_id: &str, added: usize) -> Result<(), QueueError> {
        let owned = self.owned_count(work_id);
        if owned
            .checked_add(added)
            .is_none_or(|total| total > self.limits.max_pending_per_work)
        {
            return Err(QueueError::DepthExceeded {
                work_id: work_id.to_owned(),
                limit: self.limits.max_pending_per_work,
            });
        }
        Ok(())
    }

    fn release_retry(&mut self, mut item: QueueItem, completion: QueueCompletion) -> QueueRelease {
        let retry_count = match item.retry_count.checked_add(1) {
            Some(count) => count,
            None => {
                self.known_ids.remove(&item.id);
                return rejected(
                    completion.claim_id,
                    CompletionRejection::TimeOverflow,
                    Some(item),
                );
            }
        };
        if retry_count > self.limits.max_retries {
            self.known_ids.remove(&item.id);
            return QueueRelease::DeadLettered { item, retry_count };
        }

        let delay = retry_delay(&self.limits, retry_count);
        let Some(available_at) = completion.observed_at.checked_add_signed(delay) else {
            self.known_ids.remove(&item.id);
            return rejected(
                completion.claim_id,
                CompletionRejection::TimeOverflow,
                Some(item),
            );
        };
        item.retry_count = retry_count;
        item.not_before = available_at;
        self.requeue_existing(item.clone());
        QueueRelease::Requeued {
            item,
            available_at,
            retry_count,
        }
    }

    fn take_claim(&mut self, work_id: &str) -> Option<QueueClaim> {
        let claim = self.inflight_by_work.remove(work_id)?;
        self.decrement_agent(&claim.agent_id);
        Some(claim)
    }

    fn expired_release(&mut self, mut claim: QueueClaim) -> QueueRelease {
        let withheld = self
            .withheld_inputs
            .remove(&claim.item.id)
            .unwrap_or_default();
        merge_inputs(&mut claim.item.inputs, withheld);
        self.known_ids.remove(&claim.item.id);
        QueueRelease::Expired {
            item: claim.item,
            deadline: claim.deadline,
        }
    }

    fn requeue_existing(&mut self, item: QueueItem) {
        insert_sorted(self.per_work.entry(item.work_id.clone()).or_default(), item);
    }

    fn decrement_agent(&mut self, agent_id: &str) {
        let remove = match self.inflight_by_agent.get_mut(agent_id) {
            Some(count) if *count > 1 => {
                *count -= 1;
                false
            }
            Some(_) => true,
            None => false,
        };
        if remove {
            self.inflight_by_agent.remove(agent_id);
        }
    }
}

fn validate_limits(limits: &QueueLimits) -> Result<(), QueueError> {
    const MAX_SAFE_LIMIT: usize = u32::MAX as usize;
    for (name, value) in [
        ("max_pending_per_work", limits.max_pending_per_work),
        ("max_batch_size", limits.max_batch_size),
        ("global_parallelism", limits.global_parallelism),
        (
            "default_agent_parallelism",
            limits.default_agent_parallelism,
        ),
    ] {
        if value == 0 || value > MAX_SAFE_LIMIT {
            return Err(QueueError::InvalidLimits(name));
        }
    }
    if limits.in_flight_timeout <= TimeDelta::zero()
        || limits.in_flight_timeout > TimeDelta::seconds(MAX_IN_FLIGHT_TIMEOUT_SECONDS)
    {
        return Err(QueueError::InvalidLimits("in_flight_timeout"));
    }
    if limits.max_retries > 63 {
        return Err(QueueError::InvalidLimits("max_retries"));
    }
    if limits.retry_base <= TimeDelta::zero() {
        return Err(QueueError::InvalidLimits("retry_base"));
    }
    if limits.retry_base.num_microseconds().is_none() {
        return Err(QueueError::InvalidLimits("retry_base"));
    }
    if limits.retry_max < limits.retry_base {
        return Err(QueueError::InvalidLimits("retry_max"));
    }
    if limits.retry_max.num_microseconds().is_none() {
        return Err(QueueError::InvalidLimits("retry_max"));
    }
    for (agent_id, limit) in &limits.agent_parallelism {
        if agent_id.trim().is_empty() || *limit == 0 || *limit > MAX_SAFE_LIMIT {
            return Err(QueueError::InvalidLimits("agent_parallelism"));
        }
    }
    Ok(())
}

fn validate_item(item: &mut QueueItem, limits: &QueueLimits) -> Result<(), QueueError> {
    for (field, value) in [
        ("id", item.id.as_str()),
        ("work_id", item.work_id.as_str()),
        ("agent_id", item.agent_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(QueueError::InvalidItem { field });
        }
    }
    if item.not_before < item.created_at {
        return Err(QueueError::InvalidItem {
            field: "not_before",
        });
    }
    if item.retry_count > limits.max_retries {
        return Err(QueueError::InvalidItem {
            field: "retry_count",
        });
    }
    if item.inputs.is_empty() {
        return Err(QueueError::InvalidItem { field: "inputs" });
    }
    if item.inputs.len() > limits.max_batch_size {
        return Err(QueueError::BatchExceeded {
            assignment_id: item.id.clone(),
            limit: limits.max_batch_size,
        });
    }
    let mut input_ids = BTreeSet::new();
    for input in &item.inputs {
        validate_input(input)?;
        if !input_ids.insert(input.id.as_str()) {
            return Err(QueueError::InvalidInput { field: "id" });
        }
    }
    item.inputs.sort_by(compare_inputs);
    Ok(())
}

fn validate_input(input: &QueueInput) -> Result<(), QueueError> {
    if input.id.trim().is_empty() {
        return Err(QueueError::InvalidInput { field: "id" });
    }
    Ok(())
}

fn ensure_input_capacity_and_uniqueness(
    assignment_id: &str,
    existing: &[QueueInput],
    withheld: &[QueueInput],
    input: &QueueInput,
    limit: usize,
) -> Result<(), QueueError> {
    if existing
        .len()
        .checked_add(withheld.len())
        .and_then(|count| count.checked_add(1))
        .is_none_or(|count| count > limit)
    {
        return Err(QueueError::BatchExceeded {
            assignment_id: assignment_id.to_owned(),
            limit,
        });
    }
    if existing
        .iter()
        .chain(withheld)
        .any(|existing| existing.id == input.id)
    {
        return Err(QueueError::InvalidInput { field: "id" });
    }
    Ok(())
}

fn compare_items(left: &QueueItem, right: &QueueItem) -> Ordering {
    left.created_at
        .cmp(&right.created_at)
        .then_with(|| left.id.cmp(&right.id))
}

fn insert_sorted(queue: &mut VecDeque<QueueItem>, item: QueueItem) {
    let position = queue
        .iter()
        .position(|existing| compare_items(&item, existing).is_lt())
        .unwrap_or(queue.len());
    queue.insert(position, item);
}

fn compare_inputs(left: &QueueInput, right: &QueueInput) -> Ordering {
    left.created_at
        .cmp(&right.created_at)
        .then_with(|| left.id.cmp(&right.id))
}

fn insert_input_sorted(inputs: &mut Vec<QueueInput>, input: QueueInput) {
    let position = inputs
        .iter()
        .position(|existing| compare_inputs(&input, existing).is_lt())
        .unwrap_or(inputs.len());
    inputs.insert(position, input);
}

fn merge_inputs(inputs: &mut Vec<QueueInput>, additional: Vec<QueueInput>) {
    inputs.extend(additional);
    inputs.sort_by(compare_inputs);
}

fn retry_delay(limits: &QueueLimits, retry_count: u32) -> TimeDelta {
    let base_micros = limits
        .retry_base
        .num_microseconds()
        .expect("positive TimeDelta has a bounded microsecond representation");
    let max_micros = limits
        .retry_max
        .num_microseconds()
        .expect("positive TimeDelta has a bounded microsecond representation");
    let multiplier = 1_i64
        .checked_shl(retry_count.saturating_sub(1))
        .unwrap_or(i64::MAX);
    let delay_micros = base_micros
        .checked_mul(multiplier)
        .unwrap_or(i64::MAX)
        .min(max_micros);
    TimeDelta::microseconds(delay_micros)
}

fn rejected(
    claim_id: String,
    reason: CompletionRejection,
    item: Option<QueueItem>,
) -> QueueRelease {
    QueueRelease::Rejected {
        claim_id,
        reason,
        item,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn instant(second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 16, 12, 0, second)
            .single()
            .expect("valid test timestamp")
    }

    fn item(id: &str, work_id: &str, agent_id: &str, created_at: DateTime<Utc>) -> QueueItem {
        QueueItem {
            id: id.into(),
            work_id: work_id.into(),
            agent_id: agent_id.into(),
            created_at,
            not_before: created_at,
            retry_count: 0,
            inputs: vec![QueueInput {
                id: format!("input:{id}"),
                created_at,
            }],
        }
    }

    fn input(id: &str, created_at: DateTime<Utc>) -> QueueInput {
        QueueInput {
            id: id.into(),
            created_at,
        }
    }

    fn limits() -> QueueLimits {
        QueueLimits {
            max_pending_per_work: 8,
            max_batch_size: 1,
            global_parallelism: 4,
            default_agent_parallelism: 4,
            agent_parallelism: BTreeMap::new(),
            in_flight_timeout: TimeDelta::seconds(30),
            max_retries: 2,
            retry_base: TimeDelta::seconds(5),
            retry_max: TimeDelta::seconds(20),
        }
    }

    fn complete(
        claim: &QueueClaim,
        completed_at: DateTime<Utc>,
        outcome: QueueOutcome,
    ) -> QueueCompletion {
        QueueCompletion {
            claim_id: claim.id.clone(),
            completed_at,
            observed_at: completed_at,
            outcome,
        }
    }

    #[test]
    fn oldest_head_is_fair_across_works() {
        let at = instant(0);
        let mut queue = WorkQueue::hydrate(
            [
                item("z-older", "work-a", "agent-a", at),
                item("a-tie-break", "work-b", "agent-b", at),
                item("newer", "work-c", "agent-c", instant(1)),
            ],
            limits(),
        )
        .expect("valid queue");

        let first = queue.next_claim(instant(2)).expect("oldest claim");
        assert_eq!(first.work_id, "work-b", "id breaks equal-time ties");
        let second = queue.next_claim(instant(2)).expect("second claim");
        assert_eq!(second.work_id, "work-a");
        let third = queue.next_claim(instant(2)).expect("third claim");
        assert_eq!(third.work_id, "work-c");
    }

    #[test]
    fn one_work_has_only_one_inflight_item() {
        let mut queue = WorkQueue::hydrate(
            [
                item("first", "work-a", "agent-a", instant(0)),
                item("second", "work-a", "agent-a", instant(1)),
            ],
            limits(),
        )
        .expect("valid queue");

        let first = queue.next_claim(instant(2)).expect("first item");
        assert_eq!(first.item.id, "first");
        assert!(queue.next_claim(instant(2)).is_none());
        assert!(matches!(
            queue.release(complete(&first, instant(3), QueueOutcome::Completed)),
            QueueRelease::Completed { .. }
        ));
        assert_eq!(
            queue.next_claim(instant(3)).expect("second item").item.id,
            "second"
        );
    }

    #[test]
    fn global_and_agent_parallelism_caps_are_enforced() {
        let mut configured = limits();
        configured.global_parallelism = 2;
        configured.default_agent_parallelism = 2;
        configured.agent_parallelism.insert("agent-a".into(), 1);
        let mut queue = WorkQueue::hydrate(
            [
                item("a-1", "work-a", "agent-a", instant(0)),
                item("a-2", "work-b", "agent-a", instant(1)),
                item("b-1", "work-c", "agent-b", instant(2)),
                item("b-2", "work-d", "agent-b", instant(3)),
            ],
            configured,
        )
        .expect("valid queue");

        let agent_a = queue.next_claim(instant(4)).expect("agent-a claim");
        assert_eq!(agent_a.item.id, "a-1");
        let agent_b = queue.next_claim(instant(4)).expect("agent-b claim");
        assert_eq!(agent_b.item.id, "b-1", "blocked agent is skipped");
        assert!(queue.next_claim(instant(4)).is_none(), "global cap is full");

        queue.release(complete(&agent_a, instant(5), QueueOutcome::Completed));
        assert_eq!(
            queue
                .next_claim(instant(5))
                .expect("agent-a capacity released")
                .item
                .id,
            "a-2"
        );
    }

    #[test]
    fn queue_depth_and_batch_caps_fail_closed() {
        let mut configured = limits();
        configured.max_pending_per_work = 2;
        configured.max_batch_size = 2;
        let mut first = item("first", "work-a", "agent-a", instant(0));
        first.inputs = vec![input("input-1", instant(0)), input("input-2", instant(1))];
        let mut queue = WorkQueue::hydrate(
            [first, item("second", "work-a", "agent-a", instant(1))],
            configured.clone(),
        )
        .expect("valid queue");

        assert_eq!(
            queue.push(item("rejected", "work-a", "agent-a", instant(2))),
            Err(QueueError::DepthExceeded {
                work_id: "work-a".into(),
                limit: 2,
            })
        );
        let claim = queue.next_claim(instant(3)).expect("bounded batch");
        assert_eq!(claim.item.id, "first");
        assert_eq!(
            claim
                .item
                .inputs
                .iter()
                .map(|input| input.id.as_str())
                .collect::<Vec<_>>(),
            ["input-1", "input-2"]
        );

        let mut oversized = item("oversized", "work-b", "agent-a", instant(0));
        oversized.inputs = vec![
            input("input-1", instant(0)),
            input("input-2", instant(1)),
            input("input-3", instant(2)),
        ];
        assert!(matches!(
            WorkQueue::hydrate([oversized], configured.clone()),
            Err(QueueError::BatchExceeded { .. })
        ));

        configured.max_batch_size = 0;
        assert!(matches!(
            WorkQueue::hydrate([], configured),
            Err(QueueError::InvalidLimits("max_batch_size"))
        ));
    }

    #[test]
    fn pool_exhaustion_requeues_with_original_timestamp() {
        let mut original = item("assignment", "work-a", "agent-a", instant(0));
        original.retry_count = 1;
        let mut queue = WorkQueue::hydrate([original.clone()], limits()).expect("valid queue");
        let claim = queue.next_claim(instant(1)).expect("initial claim");

        let release = queue.release(complete(&claim, instant(2), QueueOutcome::PoolExhausted));
        assert!(matches!(
            release,
            QueueRelease::Requeued { retry_count: 1, .. }
        ));
        let reclaimed = queue.next_claim(instant(2)).expect("immediate requeue");
        assert_eq!(reclaimed.item.created_at, original.created_at);
        assert_eq!(reclaimed.item.not_before, original.not_before);
        assert_eq!(reclaimed.item.retry_count, 1);
    }

    #[test]
    fn deadline_expiry_releases_capacity_once() {
        let mut configured = limits();
        configured.global_parallelism = 1;
        let mut queue = WorkQueue::hydrate(
            [
                item("expired", "work-a", "agent-a", instant(0)),
                item("next", "work-b", "agent-b", instant(1)),
            ],
            configured,
        )
        .expect("valid queue");
        let expired = queue.next_claim(instant(2)).expect("initial claim");
        assert!(queue.next_claim(instant(31)).is_none());

        let releases = queue.expire_deadlines(instant(32));
        assert!(matches!(
            &releases[..],
            [QueueRelease::Expired { item, .. }] if item.id == "expired"
        ));
        let next = queue
            .next_claim(instant(32))
            .expect("capacity after deadline");
        assert_eq!(next.item.id, "next");
        assert!(matches!(
            queue.release(complete(&expired, instant(33), QueueOutcome::Completed)),
            QueueRelease::Rejected {
                reason: CompletionRejection::UnknownOrExpiredClaim,
                ..
            }
        ));
        assert!(
            queue.next_claim(instant(33)).is_none(),
            "stale completion did not release the live claim"
        );
        assert_eq!(queue.inflight_by_agent.values().sum::<usize>(), 1);
    }

    #[test]
    fn retry_backoff_and_dead_letter_are_monotonic() {
        let mut queue =
            WorkQueue::hydrate([item("poison", "work-a", "agent-a", instant(0))], limits())
                .expect("valid queue");

        let first = queue.next_claim(instant(1)).expect("first attempt");
        let QueueRelease::Requeued {
            available_at: retry_one,
            retry_count: 1,
            ..
        } = queue.release(complete(&first, instant(2), QueueOutcome::RetryableFailure))
        else {
            panic!("first failure should requeue")
        };
        assert_eq!(retry_one, instant(7));
        assert!(queue.next_claim(instant(6)).is_none());

        let second = queue.next_claim(retry_one).expect("second attempt");
        let QueueRelease::Requeued {
            available_at: retry_two,
            retry_count: 2,
            ..
        } = queue.release(complete(&second, retry_one, QueueOutcome::RetryableFailure))
        else {
            panic!("second failure should requeue")
        };
        assert_eq!(retry_two, instant(17));
        assert!(retry_two > retry_one);

        let third = queue.next_claim(retry_two).expect("last attempt");
        assert!(matches!(
            queue.release(complete(&third, retry_two, QueueOutcome::RetryableFailure)),
            QueueRelease::DeadLettered { retry_count: 3, .. }
        ));
        assert!(queue.next_claim(instant(30)).is_none());
    }

    #[test]
    fn cancelled_batch_merge_preserves_user_order() {
        let mut configured = limits();
        configured.max_pending_per_work = 8;
        configured.max_batch_size = 4;
        let mut assignment = item("assignment", "work-a", "agent-a", instant(0));
        assignment.inputs = vec![input("01-old", instant(0)), input("02-old", instant(1))];
        let mut queue = WorkQueue::hydrate([assignment], configured).expect("valid queue");
        let cancelled = queue.next_claim(instant(2)).expect("original batch");
        queue
            .push_input("assignment", input("03-new", instant(2)))
            .expect("new user input");

        assert!(matches!(
            queue.release(complete(&cancelled, instant(3), QueueOutcome::Cancelled)),
            QueueRelease::Cancelled { .. }
        ));
        let merged = queue.next_claim(instant(3)).expect("merged claim");
        assert_eq!(
            merged
                .item
                .inputs
                .iter()
                .map(|input| input.id.as_str())
                .collect::<Vec<_>>(),
            ["01-old", "02-old", "03-new"]
        );
    }

    #[test]
    fn hydrate_rejects_duplicate_ids_and_invalid_items() {
        let duplicate = item("same", "work-a", "agent-a", instant(0));
        assert_eq!(
            WorkQueue::hydrate([duplicate.clone(), duplicate], limits()).err(),
            Some(QueueError::DuplicateId("same".into()))
        );
        assert!(matches!(
            WorkQueue::hydrate([item("", "work-a", "agent-a", instant(0))], limits()),
            Err(QueueError::InvalidItem { field: "id" })
        ));
    }

    #[test]
    fn invalid_completion_fails_closed_without_releasing_capacity() {
        let mut configured = limits();
        configured.global_parallelism = 1;
        let mut queue = WorkQueue::hydrate(
            [
                item("one", "work-a", "agent-a", instant(0)),
                item("two", "work-b", "agent-b", instant(1)),
            ],
            configured,
        )
        .expect("valid queue");
        let claim = queue.next_claim(instant(2)).expect("claim");

        assert!(matches!(
            queue.release(QueueCompletion {
                claim_id: claim.id.clone(),
                completed_at: instant(1),
                observed_at: instant(1),
                outcome: QueueOutcome::Completed,
            }),
            QueueRelease::Rejected {
                reason: CompletionRejection::CompletionBeforeClaim,
                ..
            }
        ));
        assert!(queue.next_claim(instant(3)).is_none());
    }

    #[test]
    fn unrepresentable_retry_duration_is_rejected_during_hydration() {
        let mut configured = limits();
        configured.retry_max = TimeDelta::MAX;

        assert!(matches!(
            WorkQueue::hydrate([], configured),
            Err(QueueError::InvalidLimits("retry_max"))
        ));
    }

    #[test]
    fn cancel_work_drains_items_and_releases_capacity_once() {
        let mut configured = limits();
        configured.global_parallelism = 1;
        let mut queue = WorkQueue::hydrate(
            [
                item("a-inflight", "work-a", "agent-a", instant(0)),
                item("a-pending", "work-a", "agent-a", instant(1)),
                item("b-pending", "work-b", "agent-b", instant(2)),
            ],
            configured,
        )
        .expect("valid queue");
        let stale = queue.next_claim(instant(3)).expect("work-a claim");

        let cancelled = queue.cancel_work("work-a");
        assert_eq!(
            cancelled
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["a-inflight", "a-pending"]
        );
        assert!(queue.cancel_work("work-a").is_empty());

        let live = queue.next_claim(instant(4)).expect("capacity released");
        assert_eq!(live.work_id, "work-b");
        assert!(matches!(
            queue.release(complete(&stale, instant(5), QueueOutcome::Completed)),
            QueueRelease::Rejected {
                reason: CompletionRejection::UnknownOrExpiredClaim,
                ..
            }
        ));
        assert_eq!(queue.inflight_by_work.get("work-b"), Some(&live));
    }

    #[test]
    fn claim_contains_exactly_one_assignment() {
        let mut configured = limits();
        configured.max_batch_size = 4;
        let mut queue = WorkQueue::hydrate(
            [
                item("assignment-1", "work-a", "agent-a", instant(0)),
                item("assignment-2", "work-a", "agent-a", instant(1)),
            ],
            configured,
        )
        .expect("valid queue");

        let claim = queue.next_claim(instant(2)).expect("first assignment");
        assert_eq!(claim.item.id, "assignment-1");
        assert!(queue.next_claim(instant(2)).is_none());
    }

    #[test]
    fn late_completion_before_sweep_is_not_accepted() {
        let mut queue = WorkQueue::hydrate(
            [item("assignment", "work-a", "agent-a", instant(0))],
            limits(),
        )
        .expect("valid queue");
        let claim = queue.next_claim(instant(1)).expect("claim");

        let release = queue.release(QueueCompletion {
            claim_id: claim.id.clone(),
            completed_at: claim.deadline - TimeDelta::seconds(1),
            observed_at: claim.deadline,
            outcome: QueueOutcome::Completed,
        });
        assert!(matches!(
            release,
            QueueRelease::Expired { item, deadline }
                if item.id == "assignment" && deadline == claim.deadline
        ));
        assert!(queue.expire_deadlines(claim.deadline).is_empty());
        assert!(matches!(
            queue.release(complete(&claim, claim.deadline, QueueOutcome::Completed,)),
            QueueRelease::Rejected {
                reason: CompletionRejection::UnknownOrExpiredClaim,
                ..
            }
        ));
    }

    #[test]
    fn next_claim_does_not_silently_sweep_expired_claims() {
        let mut configured = limits();
        configured.global_parallelism = 1;
        let mut queue = WorkQueue::hydrate(
            [
                item("expired", "work-a", "agent-a", instant(0)),
                item("waiting", "work-b", "agent-b", instant(1)),
            ],
            configured,
        )
        .expect("valid queue");
        let expired = queue.next_claim(instant(2)).expect("initial claim");

        assert!(queue.next_claim(expired.deadline).is_none());
        assert_eq!(queue.inflight_by_work.get("work-a"), Some(&expired));
    }

    #[test]
    fn depth_counts_inflight_assignment_ownership() {
        let mut configured = limits();
        configured.max_pending_per_work = 2;
        let mut queue = WorkQueue::hydrate(
            [
                item("inflight", "work-a", "agent-a", instant(0)),
                item("pending", "work-a", "agent-a", instant(1)),
            ],
            configured,
        )
        .expect("valid queue");
        let _claim = queue.next_claim(instant(2)).expect("claim");

        assert!(matches!(
            queue.push(item("overflow", "work-a", "agent-a", instant(3))),
            Err(QueueError::DepthExceeded { .. })
        ));
    }

    #[test]
    fn pool_exhaustion_at_full_depth_requeues_without_loss() {
        let mut configured = limits();
        configured.max_pending_per_work = 3;
        let mut queue = WorkQueue::hydrate(
            [
                item("01-inflight", "work-a", "agent-a", instant(0)),
                item("02-pending", "work-a", "agent-a", instant(1)),
            ],
            configured,
        )
        .expect("valid queue");
        let claim = queue.next_claim(instant(2)).expect("claim");
        queue
            .push(item("03-new", "work-a", "agent-a", instant(3)))
            .expect("one free ownership slot");
        assert!(matches!(
            queue.push(item("04-overflow", "work-a", "agent-a", instant(4))),
            Err(QueueError::DepthExceeded { .. })
        ));

        assert!(matches!(
            queue.release(complete(&claim, instant(5), QueueOutcome::PoolExhausted)),
            QueueRelease::Requeued { .. }
        ));
        assert_eq!(
            queue
                .cancel_work("work-a")
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["01-inflight", "02-pending", "03-new"]
        );
    }

    #[test]
    fn cancelled_transition_at_full_depth_requeues_without_loss() {
        let mut configured = limits();
        configured.max_pending_per_work = 3;
        let mut queue = WorkQueue::hydrate(
            [
                item("01-inflight", "work-a", "agent-a", instant(0)),
                item("02-pending", "work-a", "agent-a", instant(1)),
            ],
            configured,
        )
        .expect("valid queue");
        let claim = queue.next_claim(instant(2)).expect("claim");
        queue
            .push(item("03-new", "work-a", "agent-a", instant(3)))
            .expect("one free ownership slot");

        assert!(matches!(
            queue.release(complete(&claim, instant(4), QueueOutcome::Cancelled)),
            QueueRelease::Cancelled { .. }
        ));
        assert_eq!(
            queue
                .cancel_work("work-a")
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["01-inflight", "02-pending", "03-new"]
        );
    }

    #[test]
    fn retry_transition_at_full_depth_requeues_without_loss() {
        let mut configured = limits();
        configured.max_pending_per_work = 3;
        let mut queue = WorkQueue::hydrate(
            [
                item("01-inflight", "work-a", "agent-a", instant(0)),
                item("02-pending", "work-a", "agent-a", instant(1)),
            ],
            configured,
        )
        .expect("valid queue");
        let claim = queue.next_claim(instant(2)).expect("claim");
        queue
            .push(item("03-new", "work-a", "agent-a", instant(3)))
            .expect("one free ownership slot");

        assert!(matches!(
            queue.release(complete(&claim, instant(4), QueueOutcome::RetryableFailure,)),
            QueueRelease::Requeued { .. }
        ));
        assert_eq!(
            queue
                .cancel_work("work-a")
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["01-inflight", "02-pending", "03-new"]
        );
    }

    #[test]
    fn in_flight_timeout_has_an_explicit_safe_upper_bound() {
        let mut configured = limits();
        configured.in_flight_timeout = TimeDelta::hours(24);
        assert!(WorkQueue::hydrate([], configured.clone()).is_ok());

        configured.in_flight_timeout = TimeDelta::hours(24) + TimeDelta::seconds(1);
        assert!(matches!(
            WorkQueue::hydrate([], configured.clone()),
            Err(QueueError::InvalidLimits("in_flight_timeout"))
        ));

        configured.in_flight_timeout = TimeDelta::MAX;
        assert!(matches!(
            WorkQueue::hydrate([], configured),
            Err(QueueError::InvalidLimits("in_flight_timeout"))
        ));
    }

    #[test]
    fn claim_deadline_saturates_without_losing_assignment() {
        let created_at = DateTime::<Utc>::MAX_UTC - TimeDelta::seconds(1);
        let mut queue = WorkQueue::hydrate(
            [item("assignment", "work-a", "agent-a", created_at)],
            limits(),
        )
        .expect("valid queue");

        let claim = queue
            .next_claim(created_at)
            .expect("overflow-safe claim remains observable");
        assert_eq!(claim.deadline, DateTime::<Utc>::MAX_UTC);
        assert_eq!(claim.item.id, "assignment");
    }

    #[test]
    fn deadline_sweep_emits_one_observable_expired_release() {
        let mut configured = limits();
        configured.max_batch_size = 2;
        let mut queue = WorkQueue::hydrate(
            [item("assignment", "work-a", "agent-a", instant(0))],
            configured,
        )
        .expect("valid queue");
        let claim = queue.next_claim(instant(1)).expect("claim");
        queue
            .push_input("assignment", input("input:new", instant(2)))
            .expect("withheld input");

        let releases = queue.expire_deadlines(claim.deadline);
        assert_eq!(releases.len(), 1);
        assert!(matches!(
            &releases[0],
            QueueRelease::Expired { item, deadline }
                if item.id == "assignment"
                    && item.inputs.iter().map(|input| input.id.as_str()).collect::<Vec<_>>()
                        == ["input:assignment", "input:new"]
                    && *deadline == claim.deadline
        ));
        assert!(queue.expire_deadlines(claim.deadline).is_empty());
        assert!(matches!(
            queue.release(complete(&claim, claim.deadline, QueueOutcome::Completed,)),
            QueueRelease::Rejected {
                reason: CompletionRejection::UnknownOrExpiredClaim,
                ..
            }
        ));
    }

    #[test]
    fn in_flight_input_fragments_are_bounded_before_cancel_merge() {
        let mut configured = limits();
        configured.max_batch_size = 3;
        let mut assignment = item("assignment", "work-a", "agent-a", instant(0));
        assignment.inputs = vec![input("01-old", instant(0)), input("02-old", instant(1))];
        let mut queue = WorkQueue::hydrate([assignment], configured).expect("valid queue");
        let claim = queue.next_claim(instant(2)).expect("claim");

        queue
            .push_input("assignment", input("03-new", instant(2)))
            .expect("last batch slot");
        assert!(matches!(
            queue.push_input("assignment", input("04-overflow", instant(3))),
            Err(QueueError::BatchExceeded { .. })
        ));
        assert!(matches!(
            queue.release(complete(&claim, instant(4), QueueOutcome::Cancelled)),
            QueueRelease::Cancelled { .. }
        ));
    }

    #[test]
    fn normal_completion_releases_original_ownership_and_exposes_continuation_once() {
        let mut configured = limits();
        configured.max_pending_per_work = 1;
        configured.max_batch_size = 3;
        configured.global_parallelism = 1;
        let mut queue = WorkQueue::hydrate(
            [
                item("assignment", "work-a", "agent-a", instant(0)),
                item("waiting", "work-b", "agent-b", instant(1)),
            ],
            configured,
        )
        .expect("valid queue");
        let claim = queue.next_claim(instant(2)).expect("claim");
        queue
            .push_input("assignment", input("03-later-id", instant(3)))
            .expect("first continuation input");
        queue
            .push_input("assignment", input("02-earlier-id", instant(3)))
            .expect("second continuation input");

        let release = queue.release(complete(&claim, instant(4), QueueOutcome::Completed));
        let QueueRelease::Completed {
            item: completed,
            continuation_inputs,
        } = release
        else {
            panic!("normal completion should expose its continuation inputs")
        };
        assert_eq!(completed.id, "assignment");
        assert_eq!(
            continuation_inputs
                .iter()
                .map(|input| input.id.as_str())
                .collect::<Vec<_>>(),
            ["02-earlier-id", "03-later-id"]
        );
        assert_eq!(queue.owned_count("work-a"), 0);
        assert!(!queue.inflight_by_work.contains_key("work-a"));
        assert!(!queue.known_ids.contains("assignment"));

        queue
            .push(item("replacement", "work-a", "agent-a", instant(5)))
            .expect("completion releases the Work depth slot");
        let live = queue
            .next_claim(instant(5))
            .expect("completion releases global capacity");
        assert_eq!(live.item.id, "waiting");
        assert!(matches!(
            queue.release(complete(&claim, instant(5), QueueOutcome::Completed)),
            QueueRelease::Rejected {
                reason: CompletionRejection::UnknownOrExpiredClaim,
                item: None,
                ..
            }
        ));
        assert_eq!(queue.inflight_by_work.get("work-b"), Some(&live));
        assert_eq!(queue.owned_count("work-a"), 1);
    }

    #[test]
    fn normal_completion_without_withheld_inputs_exposes_empty_continuation() {
        let mut queue = WorkQueue::hydrate(
            [item("assignment", "work-a", "agent-a", instant(0))],
            limits(),
        )
        .expect("valid queue");
        let claim = queue.next_claim(instant(1)).expect("claim");

        let QueueRelease::Completed {
            item,
            continuation_inputs,
        } = queue.release(complete(&claim, instant(2), QueueOutcome::Completed))
        else {
            panic!("normal completion should be observable")
        };
        assert_eq!(item.id, "assignment");
        assert!(continuation_inputs.is_empty());
        assert_eq!(queue.owned_count("work-a"), 0);
        assert!(!queue.known_ids.contains("assignment"));
    }

    #[test]
    fn completion_timestamp_inversion_fails_closed() {
        let mut configured = limits();
        configured.global_parallelism = 1;
        let mut queue = WorkQueue::hydrate(
            [
                item("assignment", "work-a", "agent-a", instant(0)),
                item("waiting", "work-b", "agent-b", instant(1)),
            ],
            configured,
        )
        .expect("valid queue");
        let claim = queue.next_claim(instant(2)).expect("claim");

        assert!(matches!(
            queue.release(QueueCompletion {
                claim_id: claim.id,
                completed_at: instant(4),
                observed_at: instant(3),
                outcome: QueueOutcome::Completed,
            }),
            QueueRelease::Rejected {
                reason: CompletionRejection::InvalidCompletionTimes,
                ..
            }
        ));
        assert!(queue.next_claim(instant(3)).is_none());
        assert!(queue.inflight_by_work.contains_key("work-a"));
    }
}
