# PROJECT_STATE

Last updated after Milestone 13.

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

## files changed

- `server/dsprout-satellite/src/main.rs`
- Added thin file action API layer:
- `POST /upload`
- `POST /download`
- `POST /repair` (optional)
- Reuses existing uplink behavior by invoking `dsprout-uplink` binary from satellite process.
- Returns structured JSON responses with file_id/hash/status and payload data for download.
- Preserves existing worker/shard/manifest endpoints and SQLite persistence behavior.

- `server/dsprout-satellite/Cargo.toml`
- Added `base64` dependency and tokio `process`/`fs` features for file action endpoints.

- `app/lib/satellite.ts`
- Added typed request/response models for upload/download API integration.

- `app/app/page.tsx`
- Extended nav hub with file operation routes.

- `app/app/files/page.tsx`
- Added file lookup/detail view via `/manifest` and `/locate`.

- `app/app/files/upload/page.tsx`
- Added upload form (file + optional file_id + replication_factor) posting to satellite `/upload`.

- `app/app/files/download/page.tsx`
- Added download form posting to satellite `/download` and save link for returned bytes.

- `PROJECT_STATE.md`
- Updated for Milestone 13.

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

### 4) Milestone 13 direct API checks

```bash
# upload
curl -s -X POST http://127.0.0.1:7070/upload \
  -H 'content-type: application/json' \
  -d '{"file_bytes_base64":"<BASE64_BYTES>","replication_factor":2}'

# download
curl -s -X POST http://127.0.0.1:7070/download \
  -H 'content-type: application/json' \
  -d '{"file_id":"<FILE_ID>"}'
```

## validations passed

Milestone 13 validation executed successfully:

- Backend compile passed with new file action endpoints.
- Frontend lint passed with new upload/download/file pages.
- Live HTTP endpoint test passed:
- `POST /upload` succeeded
- `POST /download` succeeded
- downloaded bytes matched uploaded input exactly.

Observed result set from validation run:
- `UPLOAD_HTTP_OK=1`
- `DOWNLOAD_HTTP_OK=1`
- `FILE_ID=ui-b7f6651e-bd30-4e9a-8753-139c0958f67f`
- `EQUAL=true`
- `CMP_OK=1`

## remaining warnings/issues

- `/upload` and `/download` currently use base64 payloads/responses for minimal UI integration; large files are not optimized.
- No auth on file action endpoints yet (intentionally out of scope for this milestone).
- Satellite file actions assume local sibling `dsprout-uplink` binary availability.
- Dashboard still does not include advanced queue/progress UX.
- No desktop packaging/cloud deployment/protocol refactors yet (intentionally out of scope).

## next milestone start guidance

When opening a new Codex session, paste this file first and ask for the next milestone only.
