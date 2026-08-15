# Third-Party Notices

## Block Buzz

Parts of PiWork's activity observer, activity projection, and in-memory
Assignment queue code are adapted from Block's Buzz project at commit
`5bf78671f45178f8de02ba18d3d321cbbf19cd1f`.

Upstream: https://github.com/block/buzz
License: Apache License 2.0
Copyright 2026 Block, Inc.
Complete license text: `licenses/Apache-2.0.txt`

Local modifications replace Buzz Relay, Nostr, Channel and ACP-specific
transport concepts with PiWork Work, Assignment, Agent, Run and SQLite
WorkEvent semantics. The queue adaptation retains per-key fairness, bounded
ordered inputs, in-flight deadlines, timestamp-preserving requeue,
retry/dead-letter, and interrupted-attempt input merge concepts. PiWork limits
each claim to one persisted Assignment, counts queued and in-flight Assignment
ownership, bounds persisted and withheld input references, and hands withheld
inputs to the persistence layer after terminal completion. It adds observable
expiry, queue-epoch claim tokens, total item/Work/global/per-Agent capacity,
direct scheduling indexes, and fail-closed validation. Attempt interruption is
distinct from terminal Work cancellation. It does not include multi-Assignment
batch claims, Buzz prompt formatting, retry jitter, drop-mode deduplication, or
native-steer transport state.
