# PROJECT_STATE

Last updated after Milestone 14.

## current architecture

- Monorepo root:
- `app/` (Next.js contributor + file operations dashboard)
- `server/` (Rust backend workspace)

- Backend crates:
- `server/dsprout-common`:
- shared identity, crypto/sharding/hash, manifest models, and worker metadata models.
- `server/dsprout-worker`:
- libp2p worker with metadata-aware registration/heartbeat and shard inventory recovery.
- `server/dsprout-uplink`:
- upload/download/repair core logic (terminal engine still source of truth).
- `server/dsprout-satellite`:
- registry/index + persistence service with SQLite; now also exposes thin HTTP file action APIs that invoke uplink logic.

- Frontend:
- workers list/detail + contributor registration (Milestone 12)
- file lookup/detail + upload form + download form (Milestone 13)
- file health dashboard metrics + repair controls on file detail page (Milestone 14)

## files changed

- `app/lib/satellite.ts`
- Added typed repair API request/response models:
- `RepairApiReq`
- `RepairApiResp`

- `app/app/files/page.tsx`
- Extended file detail view to show:
- manifest summary
- shard record count
- unique shard count
- replica min/max/avg
- worker placement summary
- health status (`healthy` / `degraded` / `under-replicated`)
- Added repair control form with server action calling `POST /repair`.
- Added repair result messages showing:
- `repaired_shards`
- `new_replicas`
- Added post-repair redirect back to `/files?file_id=...` so health data is refreshed immediately.

- `PROJECT_STATE.md`
- Updated for Milestone 14.

## commands to run

All commands below are from repository root (`dsprout`).

### 1) Build backend

```bash
cd server
cargo build -p dsprout-common -p dsprout-satellite -p dsprout-worker -p dsprout-uplink
```

### 2) Start backend

```bash
cd server
cargo run -p dsprout-satellite
cargo run -p dsprout-worker -- --profile w1 --listen /ip4/127.0.0.1/tcp/5901 --satellite-url http://127.0.0.1:7070 --device-name "W1" --owner-label "Contributor" --capacity-limit-bytes 1073741824 --enabled true
cargo run -p dsprout-worker -- --profile w2 --listen /ip4/127.0.0.1/tcp/5902 --satellite-url http://127.0.0.1:7070 --device-name "W2" --owner-label "Contributor" --capacity-limit-bytes 1073741824 --enabled true
```

### 3) Run frontend

```bash
cd app
npm install
SATELLITE_URL=http://127.0.0.1:7070 npm run dev
```

Open: `http://localhost:3000`

### 4) Milestone 14 direct API checks

```bash
# upload
curl -s -X POST http://127.0.0.1:7070/upload \
  -H 'content-type: application/json' \
  -d '{"file_bytes_base64":"<BASE64_BYTES>","replication_factor":2}'

# download
curl -s -X POST http://127.0.0.1:7070/download \
  -H 'content-type: application/json' \
  -d '{"file_id":"<FILE_ID>"}'

# repair
curl -s -X POST http://127.0.0.1:7070/repair \
  -H 'content-type: application/json' \
  -d '{"file_id":"<FILE_ID>","replication_factor":2}'
```

## validations passed

Milestone 14 validation executed successfully:

- Frontend lint passed with Milestone 14 file health + repair UI changes.
- Frontend production build passed after allowing networked build (Next.js font fetch during build).
- Repair action wiring verified against existing satellite `POST /repair` response contract:
- `target_replication_factor`
- `repaired_shards`
- `new_replicas`

## remaining warnings/issues

- `/upload` and `/download` currently use base64 payloads/responses for minimal UI integration; large files are not optimized.
- No auth on file action endpoints yet (intentionally out of scope for this milestone).
- Satellite file actions assume local sibling `dsprout-uplink` binary availability.
- File health status is computed from current `/manifest` + `/locate` data; no separate backend health endpoint exists yet.
- Dashboard still does not include advanced queue/progress UX.
- No desktop packaging/cloud deployment/protocol refactors yet (intentionally out of scope).

## next milestone start guidance

When opening a new Codex session, paste this file first and ask for the next milestone only.
