# PROJECT_STATE

Last updated after Milestone 12.

## current architecture

- Monorepo root:
- `app/` (Next.js contributor dashboard)
- `server/` (Rust backend workspace)

- Backend crates:
- `server/dsprout-common`:
- shared identity, pnet loader, crypto, sharding, hashing, libp2p protocol types, manifests, and worker metadata request/response models.
- `server/dsprout-worker`:
- libp2p worker with shard store, startup inventory re-registration, registration + heartbeat, and startup metadata fields (`device_name`, `owner_label`, `capacity_limit_bytes`, `used_bytes`, `enabled`).
- `server/dsprout-uplink`:
- upload/download/repair client with healthy worker discovery and replication.
- `server/dsprout-satellite`:
- axum registry/index with SQLite persistence for workers, shard records, and manifests.
- worker metadata now includes:
- `device_name`
- `owner_label`
- `capacity_limit_bytes`
- `used_bytes`
- `enabled`

- Frontend dashboard:
- worker list page
- worker detail/status page
- contributor registration form page
- all pages call satellite endpoints through server-side fetch / server action

## files changed

- `server/dsprout-common/src/models.rs`
- Extended `WorkerInfo` with metadata fields.
- Added shared request models: `RegisterWorkerReq`, `UpdateWorkerReq`.

- `server/dsprout-satellite/src/main.rs`
- Extended worker persistence schema for metadata fields.
- Added migration logic for existing SQLite workers table columns.
- Updated `/register_worker` to persist full metadata.
- Updated `/heartbeat` to keep compatibility and allow metadata refresh.
- Added `POST /update_worker` endpoint.
- Added `GET /worker?worker_id=...` endpoint.
- Kept `GET /workers` behavior compatible (now returns richer worker objects).

- `server/dsprout-worker/src/main.rs`
- Added worker startup args:
- `--device-name`
- `--owner-label`
- `--capacity-limit-bytes`
- `--enabled`
- Worker registration now sends metadata fields.
- Heartbeat now sends metadata + refreshed `used_bytes` from local shard scan.

- `server/dsprout-uplink/src/main.rs`
- Discovery now also filters `enabled=false` workers out.

- `app/lib/satellite.ts`
- Added shared frontend satellite types/helpers for workers/manifests/locate and HTTP helpers.

- `app/app/page.tsx`
- Home page converted to navigation hub for contributor dashboard routes.

- `app/app/workers/page.tsx`
- Worker list view with health, capacity, used space, enabled status, last_seen lag.

- `app/app/workers/[worker_id]/page.tsx`
- Worker detail/status view.

- `app/app/contributors/page.tsx`
- Contributor-facing registration form posting worker metadata to satellite.

- `PROJECT_STATE.md`
- Updated for Milestone 12.

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

### 3) Start worker with contributor metadata

```bash
cd server
cargo run -p dsprout-worker -- \
  --profile w1 \
  --listen /ip4/127.0.0.1/tcp/5801 \
  --satellite-url http://127.0.0.1:7070 \
  --device-name "Devbox-1" \
  --owner-label "Alice" \
  --capacity-limit-bytes 21474836480 \
  --enabled true
```

### 4) Run dashboard

```bash
cd app
npm install
SATELLITE_URL=http://127.0.0.1:7070 npm run dev
```

Open: `http://localhost:3000`

### 5) Direct endpoint checks

```bash
curl -s http://127.0.0.1:7070/workers
curl -s "http://127.0.0.1:7070/worker?worker_id=<worker_id>"
curl -s -X POST http://127.0.0.1:7070/update_worker \
  -H 'content-type: application/json' \
  -d '{"worker_id":"<worker_id>","owner_label":"Alice-Updated","enabled":false}'
```

## validations passed

Milestone 12 validation executed successfully:

- Backend compiles with new worker metadata model + endpoints.
- Frontend lint passes with new dashboard pages.
- Integration checks passed for worker metadata APIs:
- list workers (`GET /workers`)
- get worker details (`GET /worker?worker_id=...`)
- update worker metadata (`POST /update_worker`)
- worker startup registration sends metadata fields.

Observed result set from integration run:
- `WORKER_ID=12D3KooWEDTkTbsz9zGNvaFXFL2hCBcWqbhJCWZvEKVziFV5HwC8`
- `LIST_WORKERS_OK=1`
- `GET_WORKER_OK=1`
- `UPDATE_WORKER_OK=1`
- `REGISTRATION_METADATA_FLOW_OK=1`

## remaining warnings/issues

- Contributor form currently registers/updates worker metadata only; no auth or contributor identity model yet.
- Dashboard does not yet include upload/download/repair actions (intentionally out of scope).
- Worker health remains lag-based from `last_seen` and fixed threshold in UI/backend flows.
- No Tauri/native packaging yet (intentionally out of scope).
- No Kademlia/gossipsub/cloud deployment/performance tuning yet (intentionally out of scope).

## next milestone start guidance

When opening a new Codex session, paste this file first and ask for the next milestone only.
