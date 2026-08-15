//! Per-Work Assignment queue state machine.
//!
//! Upstream path: `crates/buzz-acp/src/queue.rs` in `block/buzz`
//! Commit: `5bf78671f45178f8de02ba18d3d321cbbf19cd1f`
//! PiWork differences: Channel/Event batches become Work/Assignment queue items;
//! global/per-Agent capacity and fail-closed validation are added. Relay, Nostr,
//! ACP prompt formatting, drop-mode dedup, retry jitter, and native-steer
//! transport are omitted.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use chrono::{DateTime, TimeDelta, Utc};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub id: String,
    pub work_id: String,
    pub agent_id: String,
    pub created_at: DateTime<Utc>,
    pub not_before: DateTime<Utc>,
    pub retry_count: u32,
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
    pub items: Vec<QueueItem>,
    pub cancelled_item_ids: Vec<String>,
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
    pub finished_at: DateTime<Utc>,
    pub outcome: QueueOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueRelease {
    Completed {
        items: Vec<QueueItem>,
    },
    Requeued {
        items: Vec<QueueItem>,
        available_at: DateTime<Utc>,
        retry_count: u32,
    },
    DeadLettered {
        items: Vec<QueueItem>,
        retry_count: u32,
    },
    Cancelled {
        items: Vec<QueueItem>,
    },
    Rejected {
        claim_id: String,
        reason: CompletionRejection,
        items: Vec<QueueItem>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionRejection {
    UnknownOrExpiredClaim,
    CompletionBeforeClaim,
    CapacityExceeded,
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
    #[error("pending depth for Work {work_id} reached its cap of {limit}")]
    DepthExceeded { work_id: String, limit: usize },
}

/// Pure in-memory scheduling state derived from Buzz's per-Channel queue.
pub struct WorkQueue {
    per_work: BTreeMap<String, VecDeque<QueueItem>>,
    inflight_by_work: BTreeMap<String, QueueClaim>,
    inflight_by_agent: BTreeMap<String, usize>,
    limits: QueueLimits,
    cancelled_by_work: BTreeMap<String, Vec<QueueItem>>,
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
            cancelled_by_work: BTreeMap::new(),
            known_ids: BTreeSet::new(),
            next_claim_sequence: 0,
        };
        for item in items {
            queue.push(item)?;
        }
        Ok(queue)
    }

    pub fn push(&mut self, item: QueueItem) -> Result<(), QueueError> {
        validate_item(&item, &self.limits)?;
        if self.known_ids.contains(&item.id) {
            return Err(QueueError::DuplicateId(item.id));
        }
        self.ensure_pending_capacity(&item.work_id, 1)?;

        self.known_ids.insert(item.id.clone());
        insert_sorted(self.per_work.entry(item.work_id.clone()).or_default(), item);
        Ok(())
    }

    pub fn next_claim(&mut self, now: DateTime<Utc>) -> Option<QueueClaim> {
        self.expire_claims(now);
        if self.inflight_by_work.len() >= self.limits.global_parallelism {
            return None;
        }

        let candidate = self
            .per_work
            .keys()
            .chain(self.cancelled_by_work.keys())
            .filter(|work_id| !self.inflight_by_work.contains_key(*work_id))
            .filter_map(|work_id| {
                let head = self.work_head(work_id)?;
                if head.not_before > now || !self.agent_has_capacity(&head.agent_id) {
                    return None;
                }
                Some((work_id.clone(), head.clone()))
            })
            .min_by(|(_, left), (_, right)| compare_items(left, right));
        let (work_id, head) = candidate?;
        let next_sequence = self.next_claim_sequence.checked_add(1)?;
        let deadline = now.checked_add_signed(self.limits.in_flight_timeout)?;

        let regular = self.per_work.remove(&work_id).unwrap_or_default();
        let cancelled = self.cancelled_by_work.remove(&work_id).unwrap_or_default();
        let has_regular_items = !regular.is_empty();
        let cancelled_ids: BTreeSet<String> =
            cancelled.iter().map(|item| item.id.clone()).collect();
        let mut combined: Vec<QueueItem> = regular.into_iter().chain(cancelled).collect();
        combined.sort_by(compare_items);

        let mut claimed = Vec::new();
        let mut claimed_cancelled_ids = Vec::new();
        let mut remainder_regular = VecDeque::new();
        let mut remainder_cancelled = Vec::new();
        let mut prefix_open = true;
        for item in combined {
            let eligible = prefix_open
                && claimed.len() < self.limits.max_batch_size
                && item.agent_id == head.agent_id
                && item.retry_count == head.retry_count
                && item.not_before <= now;
            if eligible {
                if has_regular_items && cancelled_ids.contains(&item.id) {
                    claimed_cancelled_ids.push(item.id.clone());
                }
                claimed.push(item);
            } else {
                prefix_open = false;
                if cancelled_ids.contains(&item.id) {
                    remainder_cancelled.push(item);
                } else {
                    remainder_regular.push_back(item);
                }
            }
        }
        if !remainder_regular.is_empty() {
            self.per_work.insert(work_id.clone(), remainder_regular);
        }
        if !remainder_cancelled.is_empty() {
            self.cancelled_by_work
                .insert(work_id.clone(), remainder_cancelled);
        }

        let claim = QueueClaim {
            id: format!("claim:{next_sequence}:{}", head.id),
            work_id: work_id.clone(),
            agent_id: head.agent_id.clone(),
            items: claimed,
            cancelled_item_ids: claimed_cancelled_ids,
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
                Vec::new(),
            );
        };
        let claim = self
            .inflight_by_work
            .get(&work_id)
            .expect("claim located by Work key");
        if completion.finished_at < claim.claimed_at {
            return rejected(
                completion.claim_id,
                CompletionRejection::CompletionBeforeClaim,
                Vec::new(),
            );
        }

        let claim = self
            .inflight_by_work
            .remove(&work_id)
            .expect("claim still exists after validation");
        self.decrement_agent(&claim.agent_id);

        match completion.outcome {
            QueueOutcome::Completed => {
                self.forget_items(&claim.items);
                QueueRelease::Completed { items: claim.items }
            }
            QueueOutcome::PoolExhausted => {
                if !self.can_restore(&claim) {
                    self.forget_items(&claim.items);
                    return rejected(
                        completion.claim_id,
                        CompletionRejection::CapacityExceeded,
                        claim.items,
                    );
                }
                let items = claim.items.clone();
                let available_at = items
                    .iter()
                    .map(|item| item.not_before)
                    .max()
                    .unwrap_or(completion.finished_at);
                let retry_count = items.first().map_or(0, |item| item.retry_count);
                self.restore_claim(claim);
                QueueRelease::Requeued {
                    items,
                    available_at,
                    retry_count,
                }
            }
            QueueOutcome::RetryableFailure => self.release_retry(claim, completion),
            QueueOutcome::Cancelled => {
                if !self.can_restore(&claim) {
                    self.forget_items(&claim.items);
                    return rejected(
                        completion.claim_id,
                        CompletionRejection::CapacityExceeded,
                        claim.items,
                    );
                }
                let items = claim.items;
                let mut merged = self.cancelled_by_work.remove(&work_id).unwrap_or_default();
                merged.extend(items.iter().cloned());
                merged.sort_by(compare_items);
                self.cancelled_by_work.insert(work_id, merged);
                QueueRelease::Cancelled { items }
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
        cancelled.extend(self.cancelled_by_work.remove(work_id).unwrap_or_default());
        if let Some(claim) = self.inflight_by_work.remove(work_id) {
            self.decrement_agent(&claim.agent_id);
            cancelled.extend(claim.items);
        }
        cancelled.sort_by(compare_items);
        self.forget_items(&cancelled);
        cancelled
    }

    fn work_head(&self, work_id: &str) -> Option<&QueueItem> {
        let regular = self.per_work.get(work_id).and_then(VecDeque::front);
        let cancelled = self
            .cancelled_by_work
            .get(work_id)
            .and_then(|items| items.first());
        match (regular, cancelled) {
            (Some(left), Some(right)) => Some(if compare_items(left, right).is_le() {
                left
            } else {
                right
            }),
            (Some(item), None) | (None, Some(item)) => Some(item),
            (None, None) => None,
        }
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

    fn pending_count(&self, work_id: &str) -> usize {
        self.per_work
            .get(work_id)
            .map_or(0, VecDeque::len)
            .saturating_add(self.cancelled_by_work.get(work_id).map_or(0, Vec::len))
    }

    fn ensure_pending_capacity(&self, work_id: &str, added: usize) -> Result<(), QueueError> {
        let pending = self.pending_count(work_id);
        if pending
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

    fn can_restore(&self, claim: &QueueClaim) -> bool {
        self.pending_count(&claim.work_id)
            .checked_add(claim.items.len())
            .is_some_and(|total| total <= self.limits.max_pending_per_work)
    }

    fn restore_claim(&mut self, claim: QueueClaim) {
        let cancelled_ids: BTreeSet<&str> = claim
            .cancelled_item_ids
            .iter()
            .map(String::as_str)
            .collect();
        for item in claim.items {
            if cancelled_ids.contains(item.id.as_str()) {
                let work_id = item.work_id.clone();
                let cancelled = self.cancelled_by_work.entry(work_id).or_default();
                cancelled.push(item);
                cancelled.sort_by(compare_items);
            } else {
                let work_id = item.work_id.clone();
                insert_sorted(self.per_work.entry(work_id).or_default(), item);
            }
        }
    }

    fn release_retry(
        &mut self,
        mut claim: QueueClaim,
        completion: QueueCompletion,
    ) -> QueueRelease {
        let retry_count = match claim
            .items
            .first()
            .and_then(|item| item.retry_count.checked_add(1))
        {
            Some(count) => count,
            None => {
                self.forget_items(&claim.items);
                return rejected(
                    completion.claim_id,
                    CompletionRejection::TimeOverflow,
                    claim.items,
                );
            }
        };
        if retry_count > self.limits.max_retries {
            self.forget_items(&claim.items);
            return QueueRelease::DeadLettered {
                items: claim.items,
                retry_count,
            };
        }

        let delay = retry_delay(&self.limits, retry_count);
        let Some(available_at) = completion.finished_at.checked_add_signed(delay) else {
            self.forget_items(&claim.items);
            return rejected(
                completion.claim_id,
                CompletionRejection::TimeOverflow,
                claim.items,
            );
        };
        if !self.can_restore(&claim) {
            self.forget_items(&claim.items);
            return rejected(
                completion.claim_id,
                CompletionRejection::CapacityExceeded,
                claim.items,
            );
        }
        for item in &mut claim.items {
            item.retry_count = retry_count;
            item.not_before = available_at;
        }
        let items = claim.items.clone();
        self.restore_claim(claim);
        QueueRelease::Requeued {
            items,
            available_at,
            retry_count,
        }
    }

    fn expire_claims(&mut self, now: DateTime<Utc>) {
        let expired: Vec<String> = self
            .inflight_by_work
            .iter()
            .filter(|(_, claim)| claim.deadline <= now)
            .map(|(work_id, _)| work_id.clone())
            .collect();
        for work_id in expired {
            if let Some(claim) = self.inflight_by_work.remove(&work_id) {
                self.decrement_agent(&claim.agent_id);
                self.forget_items(&claim.items);
            }
        }
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

    fn forget_items(&mut self, items: &[QueueItem]) {
        for item in items {
            self.known_ids.remove(&item.id);
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
    if limits.in_flight_timeout <= TimeDelta::zero() {
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

fn validate_item(item: &QueueItem, limits: &QueueLimits) -> Result<(), QueueError> {
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

fn rejected(claim_id: String, reason: CompletionRejection, items: Vec<QueueItem>) -> QueueRelease {
    QueueRelease::Rejected {
        claim_id,
        reason,
        items,
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
        finished_at: DateTime<Utc>,
        outcome: QueueOutcome,
    ) -> QueueCompletion {
        QueueCompletion {
            claim_id: claim.id.clone(),
            finished_at,
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
        assert_eq!(first.items.len(), 1);
        assert!(queue.next_claim(instant(2)).is_none());
        assert!(matches!(
            queue.release(complete(&first, instant(3), QueueOutcome::Completed)),
            QueueRelease::Completed { .. }
        ));
        assert_eq!(
            queue.next_claim(instant(3)).expect("second item").items[0].id,
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
        assert_eq!(agent_a.items[0].id, "a-1");
        let agent_b = queue.next_claim(instant(4)).expect("agent-b claim");
        assert_eq!(agent_b.items[0].id, "b-1", "blocked agent is skipped");
        assert!(queue.next_claim(instant(4)).is_none(), "global cap is full");

        queue.release(complete(&agent_a, instant(5), QueueOutcome::Completed));
        assert_eq!(
            queue
                .next_claim(instant(5))
                .expect("agent-a capacity released")
                .items[0]
                .id,
            "a-2"
        );
    }

    #[test]
    fn queue_depth_and_batch_caps_fail_closed() {
        let mut configured = limits();
        configured.max_pending_per_work = 2;
        configured.max_batch_size = 2;
        let mut queue = WorkQueue::hydrate(
            [
                item("first", "work-a", "agent-a", instant(0)),
                item("second", "work-a", "agent-a", instant(1)),
            ],
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
        assert_eq!(
            claim
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );

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
        assert_eq!(reclaimed.items[0].created_at, original.created_at);
        assert_eq!(reclaimed.items[0].not_before, original.not_before);
        assert_eq!(reclaimed.items[0].retry_count, 1);
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

        let next = queue
            .next_claim(instant(32))
            .expect("capacity after deadline");
        assert_eq!(next.items[0].id, "next");
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
        let mut queue = WorkQueue::hydrate(
            [
                item("01-old", "work-a", "agent-a", instant(0)),
                item("02-old", "work-a", "agent-a", instant(1)),
            ],
            configured,
        )
        .expect("valid queue");
        let cancelled = queue.next_claim(instant(2)).expect("original batch");
        queue
            .push(item("03-new", "work-a", "agent-a", instant(2)))
            .expect("new user item");

        assert!(matches!(
            queue.release(complete(&cancelled, instant(3), QueueOutcome::Cancelled)),
            QueueRelease::Cancelled { .. }
        ));
        let merged = queue.next_claim(instant(3)).expect("merged claim");
        assert_eq!(
            merged
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["01-old", "02-old", "03-new"]
        );
        assert_eq!(merged.cancelled_item_ids, ["01-old", "02-old"]);
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
                finished_at: instant(1),
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
}
