# Buzz Upstream Map

PiWork pins its Buzz-derived activity work to
`block/buzz@5bf78671f45178f8de02ba18d3d321cbbf19cd1f` under the Apache License 2.0.

| Status | Buzz commit | Upstream path | Local path | Derived tests | PiWork differences | Last sync |
|---|---|---|---|---|---|---|
| Implemented | `5bf78671f45178f8de02ba18d3d321cbbf19cd1f` | `crates/buzz-acp/src/observer.rs` | `src-tauri/src/engine/activity_observer.rs` | `src-tauri/src/engine/activity_observer.rs` unit tests | Replays committed `WorkEventEnvelope` values; removes Channel, ACP, and agent-index transport fields. | 2026-08-09 |
| Implemented | `5bf78671f45178f8de02ba18d3d321cbbf19cd1f` | `desktop/src/features/agents/ui/agentSessionTypes.ts` | `src/features/activity/activityTypes.ts` | Typechecked through `src/features/activity/activityProjector.test.ts` | Replaces Relay, Nostr, pubkey, and ACP-only identities with Work, Run, Turn, Session, Agent, and Assignment identities. | 2026-08-09 |
| Implemented | `5bf78671f45178f8de02ba18d3d321cbbf19cd1f` | `desktop/src/features/agents/ui/agentSessionTranscript.ts` | `src/features/activity/activityProjector.ts` | `src/features/activity/activityProjector.test.ts` | Projects typed `WorkEventPayload` values; retains PiWork lifecycle, usage, liveness, artifact, validation, and raw-rail events. | 2026-08-09 |
| Planned / pending | `5bf78671f45178f8de02ba18d3d321cbbf19cd1f` | `desktop/src/features/agents/ui/agentSessionTranscriptGrouping.ts` | `src/features/activity/activityGrouping.ts` | Planned: `src/features/activity/activityGrouping.test.ts` | Will replace Channel prompt and Relay setup grouping with Session/Turn buckets and PiWork tool bursts; not implemented yet. | 2026-08-09 |
