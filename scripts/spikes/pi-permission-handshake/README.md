# Pi RPC permission-handshake spike

Standalone reproduction for issue #20. **Not** part of `cargo test` or `pnpm test`.

Decision note: [`docs/research/2026-08-30-pi-permission-handshake.md`](../../../docs/research/2026-08-30-pi-permission-handshake.md)

## Run

From the repository root, with Node >= 22 and the bundled sidecar present:

```bash
node scripts/spikes/pi-permission-handshake/run.mjs
```

The script:

1. Invokes `src-tauri/binaries/pi-sidecar/dist/piwork-pi.js --mode rpc` directly (Linux Rust `PiEngineAdapter` is fail-closed and will not spawn it).
2. Does **not** use live model credentials. A spike-only mock provider extension emits `read` / `edit` / `write` / `bash` tool calls.
3. Writes the observed contract to `recorded/latest.json`.

It does not change production `--approve`, `permission_requests`, Permission Mode semantics, policy, or schema.
