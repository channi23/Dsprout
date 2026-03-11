# PROJECT_STATE

Last updated after Milestone 15 + Next.js local upload/dev-origin hotfix.

## current architecture

- Monorepo root:
- `app/` (Next.js contributor + file operations + worker control dashboard)
- `server/` (Rust backend workspace)

- Backend crates:
- `server/dsprout-common`:
- shared identity, crypto/sharding/hash, manifest models, and worker metadata models.
- `server/dsprout-worker`:
- libp2p worker with metadata-aware registration/heartbeat and shard inventory recovery.
- `server/dsprout-uplink`:
- upload/download/repair core logic (terminal engine still source of truth).
- `server/dsprout-satellite`:
- registry/index + persistence service with SQLite and thin HTTP upload/download/repair file actions.
- `server/dsprout-agent`:
- local-first worker agent that manages `dsprout-worker` process lifecycle and exposes local control/status/storage endpoints.

- Frontend:
- workers list/detail + contributor registration (Milestone 12)
- file lookup/detail + upload form + download form (Milestone 13)
- file health dashboard metrics + repair controls on file detail page (Milestone 14)
- local worker control panel for start/stop/config/storage (Milestone 15)

## files changed

- `server/Cargo.toml`
- Added new workspace member: `dsprout-agent`.

- `server/dsprout-agent/Cargo.toml`
- New crate manifest for local worker agent service.

- `server/dsprout-agent/src/main.rs`
- Added local worker agent HTTP service with endpoints:
- `GET /status`
- `POST /start`
- `POST /stop`
- `POST /config`
- `GET /storage`
- Reuses existing `dsprout-worker` binary via child-process control (no worker logic rewrite).
- Persists local config to data dir (`agent-config.json`).
- Computes local storage summary (`used_bytes`, `hosted_shards`) from worker store files.

- `app/lib/satellite.ts`
- Added local agent base URL helper:
- `localAgentBaseUrl()` (defaults to `http://127.0.0.1:7081`)
- Added agent response/config types for UI integration.

- `app/app/agent/page.tsx`
- New contributor worker control panel page:
- show worker status
- start/stop worker
- update device name
- update capacity limit bytes
- show used bytes + hosted shard count

- `app/app/page.tsx`
- Added nav link to Worker Control Panel (`/agent`).

- `app/app/contributors/page.tsx`
- Added nav link to Worker Control Panel (`/agent`).

- `app/app/workers/page.tsx`
- Added nav link to Worker Control Panel (`/agent`).

- `app/app/workers/[worker_id]/page.tsx`
- Added nav link to Worker Control Panel (`/agent`).

- `server/Cargo.lock`
- Updated lockfile for new `dsprout-agent` crate dependencies.

- `PROJECT_STATE.md`
- Updated for Milestone 15.

- `app/next.config.ts`
- Added local-dev upload/body limits:
- `experimental.serverActions.bodySizeLimit = "50mb"`
- `experimental.proxyClientMaxBodySize = "50mb"`
- Added LAN dev origin allow-list:
- `allowedDevOrigins = ["10.64.23.40"]`

- `app/app/files/upload/page.tsx`
- Added note that Server Action file upload sizing changes are temporary and larger/production uploads should move to a dedicated Route Handler/API endpoint.

## commands to run

All commands below are from repository root (`dsprout`).

### 1) Build backend

```bash
cd server
cargo build -p dsprout-common -p dsprout-satellite -p dsprout-worker -p dsprout-uplink -p dsprout-agent
```

### 2) Start backend services

```bash
cd server
cargo run -p dsprout-satellite
cargo run -p dsprout-agent
```

Notes:
- `dsprout-agent` listens on `http://127.0.0.1:7081` by default.
- To override worker binary path for agent-managed start/stop, set `DSPROUT_WORKER_BIN`.

### 3) Run frontend

```bash
cd app
npm install
SATELLITE_URL=http://127.0.0.1:7070 LOCAL_AGENT_URL=http://127.0.0.1:7081 npm run dev
```

Open: `http://localhost:3000`

### 4) Milestone 15 direct API checks

```bash
# agent status
curl -s http://127.0.0.1:7081/status

# agent storage summary
curl -s http://127.0.0.1:7081/storage

# start worker (agent-managed)
curl -s -X POST http://127.0.0.1:7081/start \
  -H 'content-type: application/json' \
  -d '{}'

# update worker config
curl -s -X POST http://127.0.0.1:7081/config \
  -H 'content-type: application/json' \
  -d '{"device_name":"Contributor Laptop","capacity_limit_bytes":2147483648,"restart_if_running":true}'

# stop worker (agent-managed)
curl -s -X POST http://127.0.0.1:7081/stop \
  -H 'content-type: application/json' \
  -d '{}'
```

## validations passed

Milestone 15 validation executed successfully:

- Backend compile passed for new agent crate:
- `cargo build -p dsprout-agent`
- Frontend lint passed with new control panel page and links.
- Frontend production build passed after allowing networked font fetch.
- Local runtime smoke checks passed against agent endpoints:
- `GET /status` succeeded
- `GET /storage` succeeded
- `POST /start` succeeded
- `POST /stop` succeeded

## remaining warnings/issues

- `/upload` and `/download` currently use base64 payloads/responses for minimal UI integration; large files are not optimized.
- No auth on satellite file actions or local agent endpoints yet (intentionally out of scope).
- Satellite file actions assume local sibling `dsprout-uplink` binary availability.
- File health status is computed from `/manifest` + `/locate`; no dedicated backend health endpoint exists yet.
- Agent process/state is local-only and intentionally minimal (no desktop packaging, no cloud control plane yet).
- No desktop packaging/cloud deployment/protocol refactors yet (intentionally out of scope).

## troubleshooting (two-laptop LAN)

- Model:
- Shared satellite runs once on machine A.
- Each contributor machine (including machine B) runs its own local `dsprout-agent` + `dsprout-worker`.
- Frontend `/agent` controls only the local machine agent. Shared worker view is `/workers` via satellite.

- Required LAN config:
- On each contributor machine, set frontend `SATELLITE_URL` to machine A satellite URL (example: `http://192.168.1.10:7070`).
- In local agent config, `advertise_multiaddr` must be a LAN-reachable IP (example: `/ip4/192.168.1.22/tcp/5901`).
- Do not use advertise values with `127.x.x.x`, `localhost`, or `0.0.0.0`.

- Verify machine B joined:
- Open machine B `/agent`, confirm worker is running and `identity match` is `yes`.
- Use `/contributors` on machine B and click `Register from Local Agent`.
- Open machine A `/workers`; confirm machine B worker appears with fresh `last_seen_lag` and correct multiaddr.
- Run an upload with `replication_factor=2`; confirm it does not fail with discovery/replication errors.

- Common failures and meanings:
- `satellite preflight failed`: agent cannot reach configured satellite URL.
- `multiaddr cannot be loopback...`: worker is advertising non-LAN address; update advertise multiaddr.
- `identity match: no`: local agent `worker_id` differs from satellite record for that entry.
- `no healthy workers discovered...`: workers are stale/disabled/invalid address, or heartbeats are failing.
- `replication factor X exceeds connected workers Y`: not enough reachable healthy workers for requested replication.

## next milestone start guidance

When opening a new Codex session, paste this file first and ask for the next milestone only.
