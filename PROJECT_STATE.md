# PROJECT_STATE

Last updated after Milestone 10.

## current architecture

- Monorepo root:
- `app/` (Next.js frontend, not integrated yet)
- `server/` (Rust backend workspace)

- Backend crates:
- `server/dsprout-common`:
- shared identity, pnet loader, crypto, sharding, hashing, libp2p request/response protocol, shared durable manifest/signed-manifest models.
- `server/dsprout-worker`:
- libp2p worker node with profile-scoped local shard storage, RAM hot-cache, shard store/prepare/verify handlers, startup shard inventory scan + re-registration, registration + heartbeat.
- `server/dsprout-uplink`:
- libp2p client with satellite-driven upload worker discovery, health filtering, shard replication, signed manifest handling, download from `/locate`, and new file-scoped shard repair/re-replication command.
- `server/dsprout-satellite`:
- axum registry/index service for workers, shard locations, and signed manifests with SQLite persistence and startup reload.

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
- Persisted workers, shard records, signed manifests
- Startup restores in-memory maps from SQLite
- Shard registration/upsert is duplicate-safe

## files changed

- `server/dsprout-uplink/src/main.rs`
- Added `repair` subcommand (`dsprout-uplink repair --file-id ... --replication-factor ...`).
- Repair flow:
- queries `/locate` for file shard locations
- groups records by shard identity `(segment_index, shard_index)`
- computes healthy replica count per shard using healthy/reachable workers from `/workers`
- for under-replicated shards, fetches bytes from one healthy source replica
- chooses healthy target workers that do not already store that shard
- stores shard on targets with `StoreShard`
- registers new shard locations to satellite with `/register_shard`
- Added helper for healthy worker discovery via `last_seen` threshold.

- `PROJECT_STATE.md`
- Updated architecture, commands, and Milestone 10 validation.

## commands to run

All commands below are from repository root (`dsprout`).

### 1) Build backend

```bash
cd server
cargo build -p dsprout-common -p dsprout-satellite -p dsprout-worker -p dsprout-uplink
```

### 2) Start satellite and workers

```bash
cd server
cargo run -p dsprout-satellite
cargo run -p dsprout-worker -- --profile w1 --listen /ip4/127.0.0.1/tcp/5701 --satellite-url http://127.0.0.1:7070
cargo run -p dsprout-worker -- --profile w2 --listen /ip4/127.0.0.1/tcp/5702 --satellite-url http://127.0.0.1:7070
cargo run -p dsprout-worker -- --profile w3 --listen /ip4/127.0.0.1/tcp/5703 --satellite-url http://127.0.0.1:7070
cargo run -p dsprout-worker -- --profile w4 --listen /ip4/127.0.0.1/tcp/5704 --satellite-url http://127.0.0.1:7070
cargo run -p dsprout-worker -- --profile w5 --listen /ip4/127.0.0.1/tcp/5705 --satellite-url http://127.0.0.1:7070
```

### 3) Upload with replication factor 2

```bash
cd server
cargo run -p dsprout-uplink -- upload \
  --satellite-url http://127.0.0.1:7070 \
  --input /tmp/input.bin \
  --file-id milestone10-e2e \
  --replication-factor 2
```

### 4) Simulate replica loss (offline worker)

```bash
# stop one worker process (example: w1), then wait > 30s so it becomes unhealthy
```

### 5) Run repair / re-replication

```bash
cd server
cargo run -p dsprout-uplink -- repair \
  --satellite-url http://127.0.0.1:7070 \
  --file-id milestone10-e2e \
  --replication-factor 2
```

### 6) Download after repair and verify

```bash
cd server
cargo run -p dsprout-uplink -- download \
  --satellite-url http://127.0.0.1:7070 \
  --file-id milestone10-e2e \
  --output /tmp/output.bin
cmp -s /tmp/input.bin /tmp/output.bin && echo "MATCH" || echo "MISMATCH"
```

## validations passed

Milestone 10 validation executed successfully:

- Upload with `replication_factor=2` succeeded.
- One worker containing replicas was taken offline.
- Repair command ran and created new replicas on healthy workers.
- Healthy minimum replica count per shard was restored from 1 to 2.
- Download after repair succeeded.
- Restored file matched original exactly (`cmp` success).

Observed result set from validation run:
- `FILE_ID=milestone10-e2e-1772968696`
- `UPLOAD_REP2_OK=1`
- `WORKER_OFFLINE=1`
- `HEALTHY_MIN_REPLICAS_BEFORE_REPAIR=1`
- `REPAIR_RUN_OK=1`
- `HEALTHY_MIN_REPLICAS_AFTER_REPAIR=2`
- `REPAIRED_SHARDS=32`
- `NEW_REPLICAS=32`
- `DOWNLOAD_AFTER_REPAIR_OK=1`
- `CMP_OK=1`

## remaining warnings/issues

- Repair is file-scoped and best-effort; no scheduler/background repair loop yet.
- Healthy threshold is fixed at 30 seconds in uplink (`WORKER_HEALTH_MAX_AGE_MS`).
- Repair currently reuses immediate connectivity checks; no advanced retry/backoff strategy.
- SQLite writes are synchronous and optimized for simplicity.
- No Kademlia/bootstrap/gossipsub/cloud deployment/performance optimization yet (intentionally out of scope).

## next milestone start guidance

When opening a new Codex session, paste this file first and ask for the next milestone only.
