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
batches, in-flight deadlines, timestamp-preserving requeue, retry/dead-letter,
and cancelled-batch merge concepts while adding PiWork capacity limits and
fail-closed validation; it does not include Buzz prompt formatting, retry
jitter, drop-mode deduplication, or native-steer transport state.
