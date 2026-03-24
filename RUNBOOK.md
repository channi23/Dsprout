# DSprout Runbook

This file is the practical setup guide for running DSprout across laptops on the same LAN.

Use this when you want to:
- start the full stack
- onboard another laptop
- understand what each service does
- verify the system is healthy
- avoid the common LAN/IP mistakes

## 0. Fast Start

If you want the whole local stack to start with one command from repo root:

```bash
./start_dsprout.sh
```

This script will:
- detect your current LAN IP
- update `app/.env.local`
- start satellite
- start local agent
- update worker advertise address to the current LAN IP
- start the worker
- start the frontend

To stop everything:

```bash
./stop_dsprout.sh
```

Logs are written to:

```text
.dsprout/logs/
```

## 1. What Each Part Does

### `app/` - Next.js frontend
- The dashboard UI.
- Used to:
  - see workers
  - register contributors
  - control the local worker through the local agent
  - upload, download, inspect, and repair files

### `server/dsprout-satellite`
- The shared registry/index service.
- This is the central coordination point for all contributors on the LAN.
- Stores:
  - worker metadata
  - shard placement metadata
  - manifests
- Other laptops should point to this machine's LAN IP.

### `server/dsprout-agent`
- Local control service for one laptop.
- Runs only on that laptop.
- Used to:
  - start the worker
  - stop the worker
  - persist worker config
  - expose local status and storage info
- Default local URL:
  - `http://127.0.0.1:7081`

### `server/dsprout-worker`
- The actual storage worker process.
- Stores shards locally and responds to network requests.
- Registers itself with the satellite and sends heartbeats.
- Must advertise a LAN-reachable multiaddr.

### `server/dsprout-uplink`
- CLI engine used by satellite-backed upload, download, and repair flows.
- Talks to the satellite and dials workers to move shard data.

## 2. DSprout Model

There are two roles:

### Machine A
- Runs the shared satellite.
- Can also run its own local agent + worker.

### Machine B, C, D...
- Each contributor laptop runs:
  - its own local agent
  - its own worker
- All of them point to machine A's satellite.

Important:
- `/agent` controls only the current laptop.
- `/workers` shows the shared satellite registry.

## 3. How To Find Your LAN IP

On macOS, check your current LAN IP with:

```bash
ifconfig | rg 'inet '
```

Example output:

```text
inet 127.0.0.1
inet 192.168.0.101
```

Use the `192.168.x.x` or `10.x.x.x` address on your current Wi-Fi/Ethernet network.

Do not use:
- `127.0.0.1`
- `127.0.2.2`
- `127.0.2.3`

Those are loopback-only and cannot be reached from another laptop.

Useful macOS commands:

```bash
ipconfig getifaddr en0
ipconfig getifaddr en1
```

To check if a specific old IP is still on your machine:

```bash
ifconfig | rg '10\.64\.23\.40'
```

If that prints nothing, that IP is stale and should not be used.

## 4. Important Network Rules

### Good values
- Satellite URL:
  - `http://<MACHINE_A_LAN_IP>:7070`
- Local agent URL:
  - `http://127.0.0.1:7081`
- Worker listen multiaddr:
  - `/ip4/0.0.0.0/tcp/5901`
- Worker advertise multiaddr:
  - `/ip4/<THIS_MACHINE_LAN_IP>/tcp/5901`

### Bad values
- `http://127.0.0.1:7070` on another laptop
- `/ip4/127.0.0.1/tcp/5901`
- `/ip4/0.0.0.0/tcp/5901` as advertise address
- `localhost` for cross-machine worker connectivity

## 5. First-Time Install

From repo root:

```bash
cd server
cargo build -p dsprout-common -p dsprout-satellite -p dsprout-worker -p dsprout-uplink -p dsprout-agent
```

Frontend:

```bash
cd ../app
npm install
```

## 6. Machine A Setup

Machine A is the laptop that hosts the shared satellite.

Assume machine A LAN IP is:

```text
192.168.0.101
```

### Step 1: Start satellite

```bash
cd server
cargo run -p dsprout-satellite
```

Satellite listens on:

```text
http://0.0.0.0:7070
```

Use this URL from other laptops:

```text
http://192.168.0.101:7070
```

### Step 2: Start local agent on machine A

```bash
cd server
cargo run -p dsprout-agent
```

Agent local URL:

```text
http://127.0.0.1:7081
```

### Step 3: Start frontend on machine A

Edit `app/.env.local`:

```env
SATELLITE_URL=http://192.168.0.101:7070
LOCAL_AGENT_URL=http://127.0.0.1:7081
```

Then run:

```bash
cd app
npm run dev
```

Open:

```text
http://localhost:3000
```

### Step 4: Configure machine A worker

Open:

```text
http://localhost:3000/agent
```

Set:

```text
listen_multiaddr=/ip4/0.0.0.0/tcp/5901
advertise_multiaddr=/ip4/192.168.0.101/tcp/5901
```

Then start worker from `/agent`.

### Step 5: Register from local agent

Open:

```text
http://localhost:3000/contributors
```

Click:

```text
Register from Local Agent
```

This avoids worker ID mismatch.

## 7. Machine B Setup

Machine B is another contributor laptop on the same LAN.

Assume:
- machine A satellite IP: `192.168.0.101`
- machine B LAN IP: `192.168.0.115`

### Step 1: Verify machine B LAN IP

```bash
ifconfig | rg 'inet '
```

Use the machine B LAN IP from that output.

### Step 2: Start local agent on machine B

```bash
cd server
DSPROUT_SATELLITE_URL=http://192.168.0.101:7070 cargo run -p dsprout-agent
```

### Step 3: Start frontend on machine B

Edit `app/.env.local`:

```env
SATELLITE_URL=http://192.168.0.101:7070
LOCAL_AGENT_URL=http://127.0.0.1:7081
```

Run:

```bash
cd app
npm run dev
```

### Step 4: Configure worker on machine B

Open:

```text
http://localhost:3000/agent
```

Set:

```text
listen_multiaddr=/ip4/0.0.0.0/tcp/5901
advertise_multiaddr=/ip4/192.168.0.115/tcp/5901
```

Then start the worker.

### Step 5: Register machine B worker

Open:

```text
http://localhost:3000/contributors
```

Click:

```text
Register from Local Agent
```

## 8. Required Config Summary

### Machine A frontend

```env
SATELLITE_URL=http://<MACHINE_A_IP>:7070
LOCAL_AGENT_URL=http://127.0.0.1:7081
```

### Machine B frontend

```env
SATELLITE_URL=http://<MACHINE_A_IP>:7070
LOCAL_AGENT_URL=http://127.0.0.1:7081
```

### Worker config on any machine

```text
listen_multiaddr=/ip4/0.0.0.0/tcp/5901
advertise_multiaddr=/ip4/<THIS_MACHINE_IP>/tcp/5901
```

## 9. Commands To Verify Everything

### Check satellite

```bash
curl -s http://127.0.0.1:7070/workers
```

### Check local agent

```bash
curl -s http://127.0.0.1:7081/status
```

### Check local agent storage

```bash
curl -s http://127.0.0.1:7081/storage
```

### Start worker from agent

```bash
curl -s -X POST http://127.0.0.1:7081/start \
  -H 'content-type: application/json' \
  -d '{}'
```

### Stop worker from agent

```bash
curl -s -X POST http://127.0.0.1:7081/stop \
  -H 'content-type: application/json' \
  -d '{}'
```

### Update worker config from API

```bash
curl -s -X POST http://127.0.0.1:7081/config \
  -H 'content-type: application/json' \
  -d '{
    "listen_multiaddr": "/ip4/0.0.0.0/tcp/5901",
    "advertise_multiaddr": "/ip4/192.168.0.101/tcp/5901",
    "restart_if_running": true
  }'
```

## 10. What Healthy Looks Like

### In `/agent`
- `running=true`
- `identity_match=true`
- `multiaddr_match=true`

### In `/workers`
- each active laptop appears once
- recent `last_seen`
- correct LAN IP in `multiaddr`

### For machine B joining machine A
- machine B worker appears in machine A's `/workers`
- machine B worker has its own worker ID
- machine B multiaddr is machine B's LAN IP, not machine A's

## 11. Common Failures

### `fetch failed`
Usually means:
- wrong `SATELLITE_URL`
- satellite not running
- laptop is on a different network

### `NEXT_REDIRECT`
Old UI/server-action issue. If seen again, inspect the action response path.

### `no healthy workers discovered`
Usually means:
- worker not running
- stale worker entries
- invalid advertise address
- no recent heartbeat

### `replication factor X exceeds connected workers Y`
You requested more replicas than reachable workers currently available.

### `dial failed ... 127.0.0.1` or old IP
Shard metadata or worker advertise address is stale.

### `multiaddr cannot be loopback`
You tried to use:
- `127.0.0.1`
- `localhost`
- `0.0.0.0`
as an advertised worker address

## 12. Cleanup Notes

If you previously created dummy/test workers, the satellite may still contain stale worker rows or shard records.

What matters:
- worker registry row must point to the current real LAN IP
- shard records must also point to the same real LAN IP

If registry and shard metadata disagree, download can fail even when the worker list looks correct.

## 13. Recommended Startup Order

On any machine:

1. Start satellite on machine A
2. Start local agent
3. Start frontend
4. Configure advertise/listen values in `/agent`
5. Start worker
6. Register from local agent
7. Verify in `/workers`

## 14. Quick Checklist For Another Laptop

Before you start:
- same Wi-Fi/LAN as machine A
- know machine A LAN IP
- know this laptop's LAN IP

Then:

1. Set frontend `SATELLITE_URL=http://<MACHINE_A_IP>:7070`
2. Start `dsprout-agent`
3. Open `/agent`
4. Set `listen=/ip4/0.0.0.0/tcp/5901`
5. Set `advertise=/ip4/<THIS_MACHINE_IP>/tcp/5901`
6. Start worker
7. Open `/contributors`
8. Click `Register from Local Agent`
9. Verify in `/workers`

## 15. Example Real Setup

### Machine A
- LAN IP: `192.168.0.101`
- Satellite URL for all contributors:
  - `http://192.168.0.101:7070`
- Machine A worker advertise:
  - `/ip4/192.168.0.101/tcp/5901`

### Machine B
- LAN IP: `192.168.0.115`
- Frontend satellite URL:
  - `http://192.168.0.101:7070`
- Machine B worker advertise:
  - `/ip4/192.168.0.115/tcp/5901`

## 16. Final Rule

For cross-machine DSprout:
- satellite URL uses machine A LAN IP
- each worker advertises its own machine LAN IP
- local agent always stays on `127.0.0.1:7081`
