# Third-Party Notices

## OpenConnector Catalog

PiWork includes a compact provider-metadata snapshot generated from
OpenConnector at commit `0fa2c728dfbf957735da2843ec2b8a4f3425b105`.
Only discovery metadata is bundled; provider executors and credentials are not.

Upstream: https://github.com/oomol-lab/open-connector
License: Apache License 2.0 for upstream-authored catalog source and tooling
Complete license text: `licenses/Apache-2.0.txt`

Provider names, metadata, links, and other identifying material can be owned by
their respective providers. Catalog inclusion is for interoperability and does
not imply endorsement, sponsorship, partnership, certification, or verification.

## Simple Icons

PiWork uses selected icons from Simple Icons version 16.28.0 for recognizable
connector branding.

Upstream: https://github.com/simple-icons/simple-icons
License: CC0 1.0 Universal

All brand names, trademarks, and registered trademarks belong to their
respective owners. Icon inclusion does not imply endorsement or affiliation.

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

## Pi Web Access

PiWork bundles `pi-web-access` version 0.24.0 as its built-in web search and
content retrieval extension.

Upstream: https://github.com/nicobailon/pi-web-access
License: MIT
Copyright 2025 Nico Bailon
Complete license text: bundled at
`pi-sidecar/builtin-extensions/pi-web-access/node_modules/pi-web-access/LICENSE`
