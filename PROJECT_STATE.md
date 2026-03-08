# PROJECT_STATE

Last updated after Milestone 5.

## current architecture

- Monorepo root:
- `app/` (Next.js frontend, not integrated yet)
- `server/` (Rust backend workspace)

- Backend crates:
- `server/dsprout-common`:
- shared identity, pnet loader, crypto, sharding, hashing, libp2p request/response protocol, and now shared durable manifest/signed-manifest models.
- `server/dsprout-worker`:
- libp2p worker node, local shard storage, RAM hot-cache, shard store/prepare/verify handlers, CLI profile/listen config, satellite registration/heartbeat.
- `server/dsprout-uplink`:
- libp2p client, upload/download pipeline, satellite client, round-robin multi-worker placement, multi-worker retrieval and reconstruction, local signed-manifest cache, manifest register/fetch logic.
- `server/dsprout-satellite`:
- axum registry/index service for workers, shard locations, and signed manifests.

- Network protocol (request-response over private libp2p transport):
- `Hello` / `HelloAck`
- `Prepare` / `PrepareAck`
- `VerifyGet` / `VerifyGetOk`
- `StoreShard` / `StoreShardAck`
- `Error`

- Satellite HTTP API:
- `POST /register_worker`
- `POST /heartbeat`
- `GET /workers`
- `POST /register_shard`
- `GET /locate?file_id=...`
- `POST /register_manifest`
- `GET /manifest?file_id=...`

- libp2p transport stack:
- Ed25519 identity (profile-scoped key files)
- PSK private network via `server/swarm.key`
- TCP + pnet + noise + yamux
- identify + request_response behaviours

## files changed

- `server/dsprout-common/src/models.rs`
- Added shared `FileManifest`, `ManifestSegment`, and `SignedManifest` types.
- Added manifest signing and verification helpers using uploader identity/public key.
- Added shared satellite payload types (`WorkerInfo`, `ShardRecord`, `RegisterShardReq`, `LocateResp`, `RegisterManifestReq`).

- `server/dsprout-satellite/src/main.rs`
- Refactored to use shared model types from `dsprout-common`.
- Added in-memory manifest index.
- Added `POST /register_manifest` with signature verification.
- Added `GET /manifest?file_id=...`.

- `server/dsprout-satellite/Cargo.toml`
- Added dependency on `dsprout-common`.

- `server/dsprout-uplink/src/main.rs`
- Replaced local-only manifest structs with shared signed-manifest model.
- Upload flow now signs manifest with uplink identity, stores locally, and registers to satellite.
- Download flow now tries local manifest first, else fetches from satellite, verifies signature, and continues reconstruction.

- `server/Cargo.lock`
- Updated due dependency graph changes.

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

### 3) Start multiple workers locally

```bash
cd server
cargo run -p dsprout-worker -- --profile w1 --listen /ip4/127.0.0.1/tcp/4101 --satellite-url http://127.0.0.1:7070
cargo run -p dsprout-worker -- --profile w2 --listen /ip4/127.0.0.1/tcp/4102 --satellite-url http://127.0.0.1:7070
cargo run -p dsprout-worker -- --profile w3 --listen /ip4/127.0.0.1/tcp/4103 --satellite-url http://127.0.0.1:7070
```

### 4) Upload with explicit workers (registers signed manifest)

```bash
cd server
cargo run -p dsprout-uplink -- upload \
  --satellite-url http://127.0.0.1:7070 \
  --input /tmp/input.bin \
  --file-id milestone5-e2e \
  --worker /ip4/127.0.0.1/tcp/4101 \
  --worker /ip4/127.0.0.1/tcp/4102 \
  --worker /ip4/127.0.0.1/tcp/4103
```

### 5) Download and reconstruct (local manifest if present, satellite fallback if absent)

```bash
cd server
cargo run -p dsprout-uplink -- download \
  --satellite-url http://127.0.0.1:7070 \
  --file-id milestone5-e2e \
  --output /tmp/output.bin
```

### 6) Byte equality check

```bash
cmp -s /tmp/input.bin /tmp/output.bin && echo "MATCH" || echo "MISMATCH"
```

### 7) Validate milestone-5 fallback path (delete local uplink manifest cache)

```bash
rm -f "$HOME/Library/Application Support/dsprout/uplink_meta/milestone5-e2e.json"
cd server
cargo run -p dsprout-uplink -- download \
  --satellite-url http://127.0.0.1:7070 \
  --file-id milestone5-e2e \
  --output /tmp/output.bin
cmp -s /tmp/input.bin /tmp/output.bin && echo "MATCH" || echo "MISMATCH"
```

## validations passed

Milestone 5 validation executed successfully:

- Build passed for `dsprout-common`, `dsprout-satellite`, and `dsprout-uplink`.
- Multi-worker upload succeeded and signed manifest was saved locally.
- Signed manifest registration to satellite succeeded.
- Download succeeded with local manifest present.
- Download succeeded after deleting local uplink manifest cache (manifest fetched from satellite).
- Restored file matched original exactly (`cmp` success).

Observed result set from validation run:
- `FILE_ID=milestone5-e2e-1772965973`
- `UPLOAD_OK=1`
- `LOCAL_MANIFEST_DELETED=1`
- `DOWNLOAD_OK=1`
- `CMP_OK=1`

## remaining warnings/issues

- Satellite manifest index is currently in-memory only; it is durable for client reconstruction model semantics, but not persistent across satellite restart yet.
- `server/swarm.key` is local secret material and intentionally git-ignored at repo root.
- Placement strategy is still simple round-robin, no advanced replication policy tuning yet.
- No Kademlia/bootstrap/gossipsub/discovery yet (intentionally out of scope).
- No frontend integration yet (intentionally out of scope).
- Download reconnects to workers each run (acceptable for current milestone scope).
- No advanced retry/backoff/telemetry around worker request failures yet.

## next milestone start guidance

When opening a new Codex session, paste this file first and ask for the next milestone only.
