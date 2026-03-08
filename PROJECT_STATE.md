# PROJECT_STATE

Last updated after Milestone 8.

## current architecture

- Monorepo root:
- `app/` (Next.js frontend, not integrated yet)
- `server/` (Rust backend workspace)

- Backend crates:
- `server/dsprout-common`:
- shared identity, pnet loader, crypto, sharding, hashing, libp2p request/response protocol, shared durable manifest/signed-manifest models.
- `server/dsprout-worker`:
- libp2p worker node with profile-scoped local shard storage, RAM hot-cache, shard store/prepare/verify handlers, startup shard inventory scan + re-registration, CLI profile/listen config, satellite registration/heartbeat.
- `server/dsprout-uplink`:
- libp2p client, upload/download pipeline, satellite client, multi-worker placement, shard replication (`--replication-factor`, default `2`), local signed-manifest cache, manifest register/fetch logic.
- `server/dsprout-satellite`:
- axum registry/index service for workers, shard locations, and signed manifests with SQLite-backed persistence and startup reload.

- Network protocol (request-response over private libp2p transport):
- `Hello` / `HelloAck`
- `Prepare` / `PrepareAck`
- `VerifyGet` / `VerifyGetOk`
- `StoreShard` / `StoreShardAck`
- `Error`

- Satellite HTTP API (compatible behavior retained):
- `POST /register_worker`
- `POST /heartbeat`
- `GET /workers`
- `POST /register_shard`
- `GET /locate?file_id=...`
- `POST /register_manifest`
- `GET /manifest?file_id=...`

- Satellite persistence:
- SQLite database at `~/Library/Application Support/dsprout/satellite.sqlite3`
- Persisted tables: workers, shard records, signed manifests
- Startup restores in-memory maps from SQLite
- `register_shard` is now idempotent (upsert semantics for same file/segment/shard/worker)

## files changed

- `server/dsprout-worker/src/store.rs`
- Added startup inventory scanner: `scan_local_shards()`.
- Added `DiscoveredShard` model with reconstructed metadata from file layout.
- Added profile-scoped storage root via `set_store_profile()` so workers do not share local shard directories.

- `server/dsprout-worker/src/main.rs`
- Worker startup now calls `set_store_profile(run.profile)`.
- Added `reregister_local_shards()` to compute shard hashes and call satellite `/register_shard` for discovered local shards.
- Added startup log: `Startup shard inventory re-registered: <count>`.
- Kept worker registration + heartbeat behavior compatible.

- `server/dsprout-satellite/src/main.rs`
- Added idempotent shard handling:
- SQLite upsert for shard records on conflict (`file_id, segment_index, shard_index, worker_id`).
- In-memory shard index upsert/update instead of blind append.
- Added SQLite dedup migration and unique index for shard identity.

- `server/Cargo.lock`
- Updated due dependency graph/state changes.

## commands to run

All commands below are from repository root (`dsprout`).

### 1) Build backend

```bash
cd server
cargo build -p dsprout-common -p dsprout-satellite -p dsprout-worker -p dsprout-uplink
```

### 2) Start satellite

```bash
cd server
cargo run -p dsprout-satellite
```

### 3) Start workers

```bash
cd server
cargo run -p dsprout-worker -- --profile w1 --listen /ip4/127.0.0.1/tcp/5501 --satellite-url http://127.0.0.1:7070
cargo run -p dsprout-worker -- --profile w2 --listen /ip4/127.0.0.1/tcp/5502 --satellite-url http://127.0.0.1:7070
cargo run -p dsprout-worker -- --profile w3 --listen /ip4/127.0.0.1/tcp/5503 --satellite-url http://127.0.0.1:7070
```

### 4) Upload (replication factor 2)

```bash
cd server
cargo run -p dsprout-uplink -- upload \
  --satellite-url http://127.0.0.1:7070 \
  --input /tmp/input.bin \
  --file-id milestone8-e2e \
  --replication-factor 2 \
  --worker /ip4/127.0.0.1/tcp/5501 \
  --worker /ip4/127.0.0.1/tcp/5502 \
  --worker /ip4/127.0.0.1/tcp/5503
```

### 5) Restart worker and verify re-registration path

```bash
# stop worker w1 and start it again with same profile/listen
cd server
cargo run -p dsprout-worker -- --profile w1 --listen /ip4/127.0.0.1/tcp/5501 --satellite-url http://127.0.0.1:7070
# observe: "Startup shard inventory re-registered: <count>"
```

### 6) Download and verify

```bash
cd server
cargo run -p dsprout-uplink -- download \
  --satellite-url http://127.0.0.1:7070 \
  --file-id milestone8-e2e \
  --output /tmp/output.bin
cmp -s /tmp/input.bin /tmp/output.bin && echo "MATCH" || echo "MISMATCH"
```

## validations passed

Milestone 8 validation executed successfully:

- Upload succeeded.
- Worker was stopped and restarted.
- Restarted worker scanned local storage and re-registered inventory.
- Satellite reflected restarted worker shard records after re-registration.
- Download still succeeded afterward.
- Restored file matched original exactly (`cmp` success).

Observed result set from validation run:
- `FILE_ID=milestone8-e2e-1772968072`
- `WORKER1_ID=12D3KooWRTRuw9HBNsu4eHfukxHCR9GrbUmzRyLtzTzzsQKn8ER2`
- `UPLOAD_OK=1`
- `WORKER_RESTARTED=1`
- `WORKER_REREGISTER_RECORDS=53`
- `WORKER_STARTUP_REREG_COUNT=53`
- `DOWNLOAD_AFTER_WORKER_RESTART_OK=1`
- `CMP_OK=1`

## remaining warnings/issues

- SQLite writes are synchronous and optimized for simplicity, not throughput.
- No shard compaction/retention policy yet.
- `server/swarm.key` is local secret material and intentionally git-ignored at repo root.
- No Kademlia/bootstrap/gossipsub/discovery yet (intentionally out of scope).
- No frontend integration yet (intentionally out of scope).
- No cloud deployment/performance optimization yet (intentionally out of scope).

## next milestone start guidance

When opening a new Codex session, paste this file first and ask for the next milestone only.
