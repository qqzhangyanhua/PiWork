//! Per-Work Assignment queue state machine.
//!
//! Upstream path: `crates/buzz-acp/src/queue.rs` in `block/buzz`
//! Commit: `5bf78671f45178f8de02ba18d3d321cbbf19cd1f`
//! PiWork differences: each claim owns exactly one Work/Assignment; Buzz event
//! batches become bounded, ordered input references within that Assignment.
//! Total item/Work/global/per-Agent capacity, indexed scheduling,
//! ownership-aware depth, observable expiry, and fail-closed validation are
//! added. Attempt interruption is distinct from terminal Work cancellation.
//! Relay, Nostr, ACP prompt formatting, drop-mode dedup, retry jitter, and
//! native-steer transport are omitted.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use chrono::{DateTime, TimeDelta, Utc};
use thiserror::Error;
use uuid::Uuid;

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
    pub max_total_items: usize,
    pub max_works: usize,
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
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequeueCause {
    RetryableFailure,
    PoolExhausted,
    Interrupted,
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
        cause: RequeueCause,
    },
    DeadLettered {
        item: QueueItem,
        retry_count: u32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetryPlan {
    Requeue {
        retry_count: u32,
        available_at: DateTime<Utc>,
    },
    DeadLetter {
        retry_count: u32,
    },
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
    #[error("owned queue item count reached its cap of {limit}")]
    TotalItemsExceeded { limit: usize },
    #[error("owned Work count reached its cap of {limit}")]
    WorksExceeded { limit: usize },
    #[error("assignment queue item not found: {0}")]
    UnknownAssignment(String),
    #[error("input batch for Assignment {assignment_id} reached its cap of {limit}")]
    BatchExceeded { assignment_id: String, limit: usize },
    #[error("queue input has an invalid {field}")]
    InvalidInput { field: &'static str },
}

type ReadyHeadKey = (DateTime<Utc>, String, String);

#[cfg(test)]
#[derive(Debug, Default)]
struct QueueTestMetrics {
    hydration_bucket_sorts: usize,
    assignment_index_lookups: usize,
    claim_index_lookups: usize,
    ready_candidates_examined: usize,
}

/// Pure in-memory scheduling state derived from Buzz's per-Channel queue.
pub struct WorkQueue {
    per_work: BTreeMap<String, VecDeque<QueueItem>>,
    inflight_by_work: BTreeMap<String, QueueClaim>,
    inflight_by_agent: BTreeMap<String, usize>,
    ready_heads: BTreeSet<ReadyHeadKey>,
    assignment_to_work: BTreeMap<String, String>,
    claim_to_work: BTreeMap<String, String>,
    owned_per_work: BTreeMap<String, usize>,
    limits: QueueLimits,
    withheld_inputs: BTreeMap<String, Vec<QueueInput>>,
    claim_epoch: Uuid,
    next_claim_sequence: u64,
    #[cfg(test)]
    test_metrics: QueueTestMetrics,
}

impl WorkQueue {
    pub fn hydrate(
        items: impl IntoIterator<Item = QueueItem>,
        limits: QueueLimits,
    ) -> Result<Self, QueueError> {
        validate_limits(&limits)?;
        let mut buckets: BTreeMap<String, Vec<QueueItem>> = BTreeMap::new();
        let mut assignment_to_work = BTreeMap::new();
        for mut item in items {
            if assignment_to_work.contains_key(&item.id) {
                return Err(QueueError::DuplicateId(item.id));
            }
            if assignment_to_work.len() >= limits.max_total_items {
                return Err(QueueError::TotalItemsExceeded {
                    limit: limits.max_total_items,
                });
            }
            let is_new_work = !buckets.contains_key(&item.work_id);
            if is_new_work && buckets.len() >= limits.max_works {
                return Err(QueueError::WorksExceeded {
                    limit: limits.max_works,
                });
            }
            let work_id = item.work_id.clone();
            let bucket = buckets.entry(work_id.clone()).or_default();
            if bucket.len() >= limits.max_pending_per_work {
                return Err(QueueError::DepthExceeded {
                    work_id,
                    limit: limits.max_pending_per_work,
                });
            }
            validate_item(&mut item, &limits)?;
            assignment_to_work.insert(item.id.clone(), item.work_id.clone());
            bucket.push(item);
        }

        let mut per_work = BTreeMap::new();
        for (work_id, mut bucket) in buckets {
            bucket.sort_by(compare_items);
            per_work.insert(work_id, VecDeque::from(bucket));
        }
        let owned_per_work = per_work
            .iter()
            .map(|(work_id, bucket)| (work_id.clone(), bucket.len()))
            .collect();
        let ready_heads = per_work
            .iter()
            .filter_map(|(work_id, bucket)| {
                bucket.front().map(|item| ready_head_key(work_id, item))
            })
            .collect();
        #[cfg(test)]
        let hydration_bucket_sorts = per_work.len();

        Ok(Self {
            per_work,
            inflight_by_work: BTreeMap::new(),
            inflight_by_agent: BTreeMap::new(),
            ready_heads,
            assignment_to_work,
            claim_to_work: BTreeMap::new(),
            owned_per_work,
            limits,
            withheld_inputs: BTreeMap::new(),
            claim_epoch: Uuid::new_v4(),
            next_claim_sequence: 0,
            #[cfg(test)]
            test_metrics: QueueTestMetrics {
                hydration_bucket_sorts,
                ..QueueTestMetrics::default()
            },
        })
    }

    pub fn push(&mut self, mut item: QueueItem) -> Result<(), QueueError> {
        if self.assignment_to_work.contains_key(&item.id) {
            return Err(QueueError::DuplicateId(item.id));
        }
        self.ensure_global_capacity(&item.work_id)?;
        self.ensure_ownership_capacity(&item.work_id, 1)?;
        validate_item(&mut item, &self.limits)?;

        let work_id = item.work_id.clone();
        self.remove_ready_head(&work_id);
        self.assignment_to_work
            .insert(item.id.clone(), work_id.clone());
        *self.owned_per_work.entry(work_id.clone()).or_default() += 1;
        insert_sorted(self.per_work.entry(work_id.clone()).or_default(), item);
        self.insert_ready_head(&work_id);
        Ok(())
    }

    pub fn push_input(&mut self, assignment_id: &str, input: QueueInput) -> Result<(), QueueError> {
        validate_input(&input)?;
        #[cfg(test)]
        {
            self.test_metrics.assignment_index_lookups += 1;
        }
        let work_id = self
            .assignment_to_work
            .get(assignment_id)
            .cloned()
            .ok_or_else(|| QueueError::UnknownAssignment(assignment_id.to_owned()))?;
        let pending_position = self
            .per_work
            .get(&work_id)
            .and_then(|items| items.iter().position(|item| item.id == assignment_id));
        if let Some(position) = pending_position {
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
            .get(&work_id)
            .filter(|claim| claim.item.id == assignment_id)
            .expect("Assignment index points to pending or in-flight ownership");
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

        let mut examined = 0;
        let candidate = self.ready_heads.iter().find_map(|(_, _, work_id)| {
            examined += 1;
            let head = self
                .per_work
                .get(work_id)
                .and_then(VecDeque::front)
                .expect("ready-head index points to a pending Assignment");
            (head.not_before <= now && self.agent_has_capacity(&head.agent_id))
                .then(|| work_id.clone())
        });
        #[cfg(test)]
        {
            self.test_metrics.ready_candidates_examined += examined;
        }
        let work_id = candidate?;
        let next_sequence = self.next_claim_sequence.checked_add(1)?;
        let deadline = now
            .checked_add_signed(self.limits.in_flight_timeout)
            .unwrap_or(DateTime::<Utc>::MAX_UTC);

        self.remove_ready_head(&work_id);
        let item = self
            .per_work
            .get_mut(&work_id)
            .and_then(VecDeque::pop_front)
            .expect("candidate Work has a queue head");
        if self.per_work.get(&work_id).is_some_and(VecDeque::is_empty) {
            self.per_work.remove(&work_id);
        }

        let claim = QueueClaim {
            id: format!("claim:{}:{next_sequence}:{}", self.claim_epoch, item.id),
            work_id: work_id.clone(),
            agent_id: item.agent_id.clone(),
            item,
            claimed_at: now,
            deadline,
        };
        self.next_claim_sequence = next_sequence;
        *self
            .inflight_by_agent
            .entry(claim.agent_id.clone())
            .or_default() += 1;
        self.claim_to_work.insert(claim.id.clone(), work_id.clone());
        self.inflight_by_work.insert(work_id, claim.clone());
        Some(claim)
    }

    pub fn release(&mut self, completion: QueueCompletion) -> QueueRelease {
        #[cfg(test)]
        {
            self.test_metrics.claim_index_lookups += 1;
        }
        let work_id = self.claim_to_work.get(&completion.claim_id).cloned();
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

        let retry_plan = if completion.outcome == QueueOutcome::RetryableFailure {
            match plan_retry(&self.limits, &claim.item, completion.observed_at) {
                Ok(plan) => Some(plan),
                Err(reason) => return rejected(completion.claim_id, reason, None),
            }
        } else {
            None
        };

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
                self.forget_assignment(&completed);
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
                    cause: RequeueCause::PoolExhausted,
                }
            }
            QueueOutcome::RetryableFailure => {
                merge_inputs(&mut claim.item.inputs, withheld);
                self.release_retry(
                    claim.item,
                    retry_plan.expect("retry outcome has a precomputed plan"),
                )
            }
            QueueOutcome::Interrupted => {
                merge_inputs(&mut claim.item.inputs, withheld);
                let item = claim.item;
                let available_at = item.not_before;
                let retry_count = item.retry_count;
                self.requeue_existing(item.clone());
                QueueRelease::Requeued {
                    item,
                    available_at,
                    retry_count,
                    cause: RequeueCause::Interrupted,
                }
            }
        }
    }

    pub fn cancel_work(&mut self, work_id: &str) -> Vec<QueueItem> {
        self.remove_ready_head(work_id);
        let mut cancelled: Vec<QueueItem> = self
            .per_work
            .remove(work_id)
            .unwrap_or_default()
            .into_iter()
            .collect();
        if let Some(claim) = self.take_claim(work_id) {
            let mut item = claim.item;
            let withheld = self.withheld_inputs.remove(&item.id).unwrap_or_default();
            merge_inputs(&mut item.inputs, withheld);
            cancelled.push(item);
        }
        cancelled.sort_by(compare_items);
        for item in &cancelled {
            self.withheld_inputs.remove(&item.id);
            self.assignment_to_work.remove(&item.id);
        }
        self.owned_per_work.remove(work_id);
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
        self.owned_per_work.get(work_id).copied().unwrap_or(0)
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

    fn ensure_global_capacity(&self, work_id: &str) -> Result<(), QueueError> {
        if self.assignment_to_work.len() >= self.limits.max_total_items {
            return Err(QueueError::TotalItemsExceeded {
                limit: self.limits.max_total_items,
            });
        }
        let work_is_owned = self.owned_per_work.contains_key(work_id);
        if !work_is_owned && self.owned_work_count() >= self.limits.max_works {
            return Err(QueueError::WorksExceeded {
                limit: self.limits.max_works,
            });
        }
        Ok(())
    }

    fn owned_work_count(&self) -> usize {
        self.owned_per_work.len()
    }

    fn release_retry(&mut self, mut item: QueueItem, plan: RetryPlan) -> QueueRelease {
        match plan {
            RetryPlan::DeadLetter { retry_count } => {
                self.forget_assignment(&item);
                QueueRelease::DeadLettered { item, retry_count }
            }
            RetryPlan::Requeue {
                retry_count,
                available_at,
            } => {
                item.retry_count = retry_count;
                item.not_before = available_at;
                self.requeue_existing(item.clone());
                QueueRelease::Requeued {
                    item,
                    available_at,
                    retry_count,
                    cause: RequeueCause::RetryableFailure,
                }
            }
        }
    }

    fn take_claim(&mut self, work_id: &str) -> Option<QueueClaim> {
        let claim = self.inflight_by_work.remove(work_id)?;
        self.claim_to_work.remove(&claim.id);
        self.decrement_agent(&claim.agent_id);
        Some(claim)
    }

    fn expired_release(&mut self, mut claim: QueueClaim) -> QueueRelease {
        let withheld = self
            .withheld_inputs
            .remove(&claim.item.id)
            .unwrap_or_default();
        merge_inputs(&mut claim.item.inputs, withheld);
        self.forget_assignment(&claim.item);
        QueueRelease::Expired {
            item: claim.item,
            deadline: claim.deadline,
        }
    }

    fn requeue_existing(&mut self, item: QueueItem) {
        let work_id = item.work_id.clone();
        self.remove_ready_head(&work_id);
        insert_sorted(self.per_work.entry(work_id.clone()).or_default(), item);
        self.insert_ready_head(&work_id);
    }

    fn forget_assignment(&mut self, item: &QueueItem) {
        self.assignment_to_work.remove(&item.id);
        let remove_work = match self.owned_per_work.get_mut(&item.work_id) {
            Some(count) if *count > 1 => {
                *count -= 1;
                false
            }
            Some(_) => true,
            None => false,
        };
        if remove_work {
            self.owned_per_work.remove(&item.work_id);
        }
        self.insert_ready_head(&item.work_id);
    }

    fn remove_ready_head(&mut self, work_id: &str) {
        if self.inflight_by_work.contains_key(work_id) {
            return;
        }
        if let Some(item) = self.per_work.get(work_id).and_then(VecDeque::front) {
            self.ready_heads.remove(&ready_head_key(work_id, item));
        }
    }

    fn insert_ready_head(&mut self, work_id: &str) {
        if self.inflight_by_work.contains_key(work_id) {
            return;
        }
        if let Some(item) = self.per_work.get(work_id).and_then(VecDeque::front) {
            self.ready_heads.insert(ready_head_key(work_id, item));
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
}

fn validate_limits(limits: &QueueLimits) -> Result<(), QueueError> {
    const MAX_SAFE_LIMIT: usize = u32::MAX as usize;
    for (name, value) in [
        ("max_pending_per_work", limits.max_pending_per_work),
        ("max_total_items", limits.max_total_items),
        ("max_works", limits.max_works),
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
    if exact_positive_microseconds(limits.retry_base).is_none() {
        return Err(QueueError::InvalidLimits("retry_base"));
    }
    if limits.retry_max < limits.retry_base {
        return Err(QueueError::InvalidLimits("retry_max"));
    }
    if exact_positive_microseconds(limits.retry_max).is_none() {
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

fn ready_head_key(work_id: &str, item: &QueueItem) -> ReadyHeadKey {
    (item.created_at, item.id.clone(), work_id.to_owned())
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

fn exact_positive_microseconds(duration: TimeDelta) -> Option<i64> {
    if duration <= TimeDelta::zero() {
        return None;
    }
    let microseconds = duration.num_microseconds()?;
    (TimeDelta::microseconds(microseconds) == duration).then_some(microseconds)
}

fn plan_retry(
    limits: &QueueLimits,
    item: &QueueItem,
    observed_at: DateTime<Utc>,
) -> Result<RetryPlan, CompletionRejection> {
    let retry_count = item
        .retry_count
        .checked_add(1)
        .ok_or(CompletionRejection::TimeOverflow)?;
    if retry_count > limits.max_retries {
        return Ok(RetryPlan::DeadLetter { retry_count });
    }
    let available_at = observed_at
        .checked_add_signed(retry_delay(limits, retry_count))
        .ok_or(CompletionRejection::TimeOverflow)?;
    Ok(RetryPlan::Requeue {
        retry_count,
        available_at,
    })
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
            max_total_items: 256,
            max_works: 64,
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

    fn assert_indexes_consistent(queue: &WorkQueue) {
        let mut expected_assignments = BTreeMap::new();
        let mut expected_owned_per_work = BTreeMap::new();
        for (work_id, items) in &queue.per_work {
            expected_owned_per_work.insert(work_id.clone(), items.len());
            for item in items {
                expected_assignments.insert(item.id.clone(), work_id.clone());
            }
        }
        for (work_id, claim) in &queue.inflight_by_work {
            *expected_owned_per_work.entry(work_id.clone()).or_default() += 1;
            expected_assignments.insert(claim.item.id.clone(), work_id.clone());
        }
        let expected_claims: BTreeMap<String, String> = queue
            .inflight_by_work
            .iter()
            .map(|(work_id, claim)| (claim.id.clone(), work_id.clone()))
            .collect();
        let expected_ready: BTreeSet<(DateTime<Utc>, String, String)> = queue
            .per_work
            .iter()
            .filter(|(work_id, _)| !queue.inflight_by_work.contains_key(*work_id))
            .filter_map(|(work_id, items)| {
                items
                    .front()
                    .map(|item| (item.created_at, item.id.clone(), work_id.clone()))
            })
            .collect();

        assert_eq!(queue.assignment_to_work, expected_assignments);
        assert_eq!(queue.claim_to_work, expected_claims);
        assert_eq!(queue.owned_per_work, expected_owned_per_work);
        assert_eq!(queue.ready_heads, expected_ready);
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
    fn interrupted_attempt_merge_preserves_user_order() {
        let mut configured = limits();
        configured.max_pending_per_work = 8;
        configured.max_batch_size = 4;
        let mut assignment = item("assignment", "work-a", "agent-a", instant(0));
        assignment.inputs = vec![input("01-old", instant(0)), input("02-old", instant(1))];
        let mut queue = WorkQueue::hydrate([assignment], configured).expect("valid queue");
        let interrupted = queue.next_claim(instant(2)).expect("original batch");
        queue
            .push_input("assignment", input("03-new", instant(2)))
            .expect("new user input");

        assert!(matches!(
            queue.release(complete(
                &interrupted,
                instant(3),
                QueueOutcome::Interrupted,
            )),
            QueueRelease::Requeued {
                cause: RequeueCause::Interrupted,
                ..
            }
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
    fn interrupted_transition_at_full_depth_requeues_without_loss() {
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
            queue.release(complete(&claim, instant(4), QueueOutcome::Interrupted)),
            QueueRelease::Requeued {
                cause: RequeueCause::Interrupted,
                ..
            }
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
    fn in_flight_input_fragments_are_bounded_before_interrupted_merge() {
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
            queue.release(complete(&claim, instant(4), QueueOutcome::Interrupted)),
            QueueRelease::Requeued {
                cause: RequeueCause::Interrupted,
                ..
            }
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
        assert!(!queue.assignment_to_work.contains_key("assignment"));

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
        assert!(!queue.assignment_to_work.contains_key("assignment"));
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

    #[test]
    fn stale_claim_token_from_an_earlier_hydration_cannot_release_a_new_claim() {
        let mut earlier = WorkQueue::hydrate(
            [item("assignment", "work-a", "agent-a", instant(0))],
            limits(),
        )
        .expect("earlier queue");
        let stale = earlier.next_claim(instant(1)).expect("earlier claim");

        let mut current = WorkQueue::hydrate(
            [item("assignment", "work-a", "agent-a", instant(0))],
            limits(),
        )
        .expect("current queue");
        let live = current.next_claim(instant(1)).expect("current claim");

        assert!(matches!(
            current.release(complete(&stale, instant(2), QueueOutcome::Completed)),
            QueueRelease::Rejected {
                reason: CompletionRejection::UnknownOrExpiredClaim,
                item: None,
                ..
            }
        ));
        assert_ne!(stale.id, live.id);
        assert_eq!(current.inflight_by_work.get("work-a"), Some(&live));
        assert_eq!(current.inflight_by_agent.get("agent-a"), Some(&1));
        assert!(matches!(
            current.release(complete(&live, instant(2), QueueOutcome::Completed)),
            QueueRelease::Completed { .. }
        ));
    }

    #[test]
    fn retry_time_overflow_rejection_preserves_claim_inputs_and_capacity() {
        let claimed_at = DateTime::<Utc>::MAX_UTC - TimeDelta::seconds(10);
        let observed_at = DateTime::<Utc>::MAX_UTC - TimeDelta::microseconds(1);
        let mut configured = limits();
        configured.global_parallelism = 1;
        configured.max_batch_size = 2;
        let mut queue = WorkQueue::hydrate(
            [
                item("assignment", "work-a", "agent-a", claimed_at),
                item(
                    "waiting",
                    "work-b",
                    "agent-b",
                    claimed_at + TimeDelta::seconds(1),
                ),
            ],
            configured,
        )
        .expect("valid queue");
        let claim = queue.next_claim(claimed_at).expect("claim");
        queue
            .push_input(
                "assignment",
                input("input:new", claimed_at + TimeDelta::seconds(1)),
            )
            .expect("withheld input");
        let withheld_before = queue
            .withheld_inputs
            .get("assignment")
            .cloned()
            .expect("withheld snapshot");

        assert!(matches!(
            queue.release(QueueCompletion {
                claim_id: claim.id.clone(),
                completed_at: observed_at,
                observed_at,
                outcome: QueueOutcome::RetryableFailure,
            }),
            QueueRelease::Rejected {
                reason: CompletionRejection::TimeOverflow,
                item: None,
                ..
            }
        ));
        assert_eq!(queue.inflight_by_work.get("work-a"), Some(&claim));
        assert_eq!(
            queue.withheld_inputs.get("assignment"),
            Some(&withheld_before)
        );
        assert!(queue.assignment_to_work.contains_key("assignment"));
        assert_eq!(queue.inflight_by_agent.get("agent-a"), Some(&1));
        assert!(queue.next_claim(observed_at).is_none());

        let QueueRelease::Completed {
            item,
            continuation_inputs,
        } = queue.release(QueueCompletion {
            claim_id: claim.id,
            completed_at: claimed_at + TimeDelta::seconds(2),
            observed_at: claimed_at + TimeDelta::seconds(2),
            outcome: QueueOutcome::Completed,
        })
        else {
            panic!("claim remains legally completable after rejection")
        };
        assert_eq!(item.id, "assignment");
        assert_eq!(continuation_inputs, withheld_before);
        assert_eq!(
            queue
                .next_claim(claimed_at + TimeDelta::seconds(3))
                .expect("capacity released once")
                .item
                .id,
            "waiting"
        );
    }

    #[test]
    fn retry_durations_require_exact_positive_microseconds() {
        for invalid in [TimeDelta::nanoseconds(500), TimeDelta::nanoseconds(1_500)] {
            let mut configured = limits();
            configured.retry_base = invalid;
            assert!(matches!(
                WorkQueue::hydrate([], configured),
                Err(QueueError::InvalidLimits("retry_base"))
            ));
        }

        let mut configured = limits();
        configured.retry_base = TimeDelta::microseconds(1);
        configured.retry_max = TimeDelta::nanoseconds(1_500);
        assert!(matches!(
            WorkQueue::hydrate([], configured),
            Err(QueueError::InvalidLimits("retry_max"))
        ));

        let mut configured = limits();
        configured.retry_base = TimeDelta::microseconds(1);
        configured.retry_max = TimeDelta::microseconds(1);
        let mut queue = WorkQueue::hydrate(
            [item("assignment", "work-a", "agent-a", instant(0))],
            configured,
        )
        .expect("one-microsecond retry boundary is valid");
        let claim = queue.next_claim(instant(1)).expect("claim");
        let QueueRelease::Requeued {
            available_at,
            retry_count,
            ..
        } = queue.release(complete(&claim, instant(2), QueueOutcome::RetryableFailure))
        else {
            panic!("retry boundary should remain representable")
        };
        assert_eq!(retry_count, 1);
        assert_eq!(available_at, instant(2) + TimeDelta::microseconds(1));
        assert!(available_at > instant(2));
    }

    #[test]
    fn global_item_and_work_caps_fail_closed_without_partial_insertion() {
        let mut item_limited = limits();
        item_limited.max_total_items = 2;
        item_limited.max_works = 2;
        let mut queue = WorkQueue::hydrate(
            [
                item("one", "work-a", "agent-a", instant(0)),
                item("two", "work-b", "agent-b", instant(1)),
            ],
            item_limited.clone(),
        )
        .expect("queue at total cap");
        assert_eq!(
            queue.push(item("three", "work-c", "agent-c", instant(2))),
            Err(QueueError::TotalItemsExceeded { limit: 2 })
        );
        assert_eq!(queue.assignment_to_work.len(), 2);
        assert!(!queue.assignment_to_work.contains_key("three"));
        assert!(!queue.per_work.contains_key("work-c"));
        assert!(matches!(
            WorkQueue::hydrate(
                [
                    item("one", "work-a", "agent-a", instant(0)),
                    item("two", "work-b", "agent-b", instant(1)),
                    item("three", "work-c", "agent-c", instant(2)),
                ],
                item_limited,
            ),
            Err(QueueError::TotalItemsExceeded { limit: 2 })
        ));

        let mut work_limited = limits();
        work_limited.max_total_items = 3;
        work_limited.max_works = 2;
        let mut queue = WorkQueue::hydrate(
            [
                item("one", "work-a", "agent-a", instant(0)),
                item("two", "work-b", "agent-b", instant(1)),
            ],
            work_limited.clone(),
        )
        .expect("queue at Work cap");
        assert_eq!(
            queue.push(item("three", "work-c", "agent-c", instant(2))),
            Err(QueueError::WorksExceeded { limit: 2 })
        );
        assert_eq!(queue.assignment_to_work.len(), 2);
        assert!(!queue.per_work.contains_key("work-c"));
        assert!(matches!(
            WorkQueue::hydrate(
                [
                    item("one", "work-a", "agent-a", instant(0)),
                    item("two", "work-b", "agent-b", instant(1)),
                    item("three", "work-c", "agent-c", instant(2)),
                ],
                work_limited,
            ),
            Err(QueueError::WorksExceeded { limit: 2 })
        ));

        let mut invalid = limits();
        invalid.max_total_items = 0;
        assert!(matches!(
            WorkQueue::hydrate([], invalid),
            Err(QueueError::InvalidLimits("max_total_items"))
        ));
        let mut invalid = limits();
        invalid.max_works = 0;
        assert!(matches!(
            WorkQueue::hydrate([], invalid),
            Err(QueueError::InvalidLimits("max_works"))
        ));
    }

    #[test]
    fn large_multi_work_queue_uses_consistent_direct_indexes() {
        const WORKS: usize = 64;
        const ITEMS_PER_WORK: usize = 4;
        let base = instant(0);
        let mut configured = limits();
        configured.max_total_items = WORKS * ITEMS_PER_WORK;
        configured.max_works = WORKS;
        configured.max_batch_size = 2;
        let mut items = Vec::with_capacity(WORKS * ITEMS_PER_WORK);
        for work in 0..WORKS {
            for slot in (0..ITEMS_PER_WORK).rev() {
                items.push(item(
                    &format!("assignment-{work:03}-{slot:03}"),
                    &format!("work-{work:03}"),
                    &format!("agent-{work:03}"),
                    base + TimeDelta::seconds(slot as i64),
                ));
            }
        }

        let mut queue = WorkQueue::hydrate(items, configured).expect("bounded large queue");
        assert_indexes_consistent(&queue);
        assert_eq!(queue.test_metrics.hydration_bucket_sorts, WORKS);

        queue.test_metrics.assignment_index_lookups = 0;
        queue
            .push_input(
                "assignment-063-003",
                input("input:new", base + TimeDelta::seconds(4)),
            )
            .expect("direct Assignment lookup");
        assert_eq!(queue.test_metrics.assignment_index_lookups, 1);

        queue.test_metrics.ready_candidates_examined = 0;
        let claim = queue
            .next_claim(base + TimeDelta::seconds(10))
            .expect("indexed ready head");
        assert_eq!(claim.item.id, "assignment-000-000");
        assert_eq!(queue.test_metrics.ready_candidates_examined, 1);
        assert_indexes_consistent(&queue);

        queue.test_metrics.claim_index_lookups = 0;
        assert!(matches!(
            queue.release(complete(
                &claim,
                base + TimeDelta::seconds(11),
                QueueOutcome::PoolExhausted,
            )),
            QueueRelease::Requeued { .. }
        ));
        assert_eq!(queue.test_metrics.claim_index_lookups, 1);
        assert_indexes_consistent(&queue);

        assert_eq!(queue.cancel_work("work-063").len(), ITEMS_PER_WORK);
        assert_indexes_consistent(&queue);
        assert_eq!(
            queue.assignment_to_work.len(),
            WORKS * ITEMS_PER_WORK - ITEMS_PER_WORK
        );
    }

    #[test]
    fn interrupted_attempt_requeues_while_cancel_work_is_terminal() {
        let mut configured = limits();
        configured.max_batch_size = 3;
        let mut assignment = item("assignment", "work-a", "agent-a", instant(0));
        assignment.inputs = vec![input("01-old", instant(0))];
        let mut queue = WorkQueue::hydrate([assignment], configured).expect("valid queue");
        let claim = queue.next_claim(instant(1)).expect("claim");
        queue
            .push_input("assignment", input("03-later-id", instant(2)))
            .expect("first withheld input");
        queue
            .push_input("assignment", input("02-earlier-id", instant(2)))
            .expect("second withheld input");

        assert!(matches!(
            queue.release(complete(&claim, instant(3), QueueOutcome::Interrupted)),
            QueueRelease::Requeued {
                cause: RequeueCause::Interrupted,
                ..
            }
        ));
        assert!(queue.assignment_to_work.contains_key("assignment"));
        let reclaimed = queue.next_claim(instant(3)).expect("interrupted retry");
        assert_eq!(
            reclaimed
                .item
                .inputs
                .iter()
                .map(|input| input.id.as_str())
                .collect::<Vec<_>>(),
            ["01-old", "02-earlier-id", "03-later-id"]
        );

        let cancelled = queue.cancel_work("work-a");
        assert_eq!(cancelled.len(), 1);
        assert_eq!(cancelled[0].id, "assignment");
        assert!(!queue.assignment_to_work.contains_key("assignment"));
        assert!(queue.cancel_work("work-a").is_empty());
        assert_indexes_consistent(&queue);
    }
}
