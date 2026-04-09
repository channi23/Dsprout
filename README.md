# DSprout

DSprout is a LAN-based distributed storage project.

It has five main parts:

- `server/dsprout-satellite`: the shared metadata and coordination service
- `server/dsprout-agent`: the local control service for one machine
- `server/dsprout-worker`: the storage worker process that stores shards and joins the network
- `server/dsprout-uplink`: the CLI used for upload, download, and repair flows
- `app/`: the Next.js dashboard used to control the local machine and inspect the shared network

This README explains how to run DSprout in the two most important roles:

- Machine A: the machine that hosts the shared satellite
- Machine B: another contributor machine that joins Machine A over the same LAN

This guide is intentionally manual and command-first. It does not depend on helper scripts.

## 1. Network Model

Machine A hosts the satellite on port `7070`.

Every contributor machine, including Machine B, runs:

- one local agent on `127.0.0.1:7081`
- one local worker on port `5901`
- one local frontend on port `3000`

Important rules:

- Machine B must use Machine A's LAN IP for the satellite URL
- the local agent always stays on `127.0.0.1:7081`
- every worker must advertise its own machine's LAN IP
- do not advertise `127.0.0.1`, `localhost`, or `0.0.0.0`

## 2. Prerequisites

Install the following on every machine:

- Rust and Cargo
- Node.js and npm

Clone the repository on every machine:

```bash
git clone https://github.com/channi23/Dsprout.git
cd Dsprout
```

Build the backend crates once:

```bash
cd server
cargo build -p dsprout-common -p dsprout-satellite -p dsprout-worker -p dsprout-uplink -p dsprout-agent
cd ..
```

Install frontend dependencies once:

```bash
cd app
npm install
cd ..
```

## 3. Find the Correct LAN IP

Run this on each machine:

```bash
ipconfig getifaddr en0
```

If `en0` is empty, try:

```bash
ipconfig getifaddr en1
```

You can also inspect all IPs with:

```bash
ifconfig | grep 'inet '
```

Use the real LAN IP from your current Wi-Fi or Ethernet network.

Examples of good IPs:

- `192.168.0.101`
- `10.145.113.207`

Examples of bad IPs for cross-machine access:

- `127.0.0.1`
- `localhost`
- `0.0.0.0`

## 4. Ports Used by DSprout

- `7070`: satellite
- `7081`: local agent
- `5901`: worker
- `3000`: frontend

If you already have something else using one of these ports, stop that process first or change your local setup before continuing.

## 5. Machine A Setup

Machine A is the machine that hosts the shared satellite.

In the examples below, replace:

- `<MACHINE_A_IP>` with Machine A's LAN IP

Example:

- `<MACHINE_A_IP>` -> `192.168.0.101`

### Step 1: Open three terminals on Machine A

You will use:

- Terminal 1 for the satellite
- Terminal 2 for the local agent
- Terminal 3 for the frontend

### Step 2: Start the satellite on Machine A

In Terminal 1:

```bash
cd /path/to/Dsprout/server
cargo run -p dsprout-satellite
```

Expected result:

- the process stays running
- it listens on `http://0.0.0.0:7070`
- other machines should reach it with `http://<MACHINE_A_IP>:7070`

Quick check from Machine A:

```bash
curl -s http://127.0.0.1:7070/workers
```

### Step 3: Start the local agent on Machine A

In Terminal 2:

```bash
cd /path/to/Dsprout/server
DSPROUT_SATELLITE_URL=http://<MACHINE_A_IP>:7070 cargo run -p dsprout-agent
```

Expected result:

- the process stays running
- it listens on `http://127.0.0.1:7081`

Quick check:

```bash
curl -s http://127.0.0.1:7081/status
```

### Step 4: Configure the frontend on Machine A

Create or update `app/.env.local`:

```env
SATELLITE_URL=http://<MACHINE_A_IP>:7070
LOCAL_AGENT_URL=http://127.0.0.1:7081
```

### Step 5: Start the frontend on Machine A

In Terminal 3:

```bash
cd /path/to/Dsprout/app
npm run dev
```

Open:

```text
http://localhost:3000
```

### Step 6: Configure Machine A's worker from the UI

Open:

```text
http://localhost:3000/agent
```

Set the worker fields to:

```text
listen_multiaddr=/ip4/0.0.0.0/tcp/5901
advertise_multiaddr=/ip4/<MACHINE_A_IP>/tcp/5901
satellite_url=http://<MACHINE_A_IP>:7070
```

Then start the worker from the page.

### Step 7: Register Machine A from the local agent

Open:

```text
http://localhost:3000/contributors
```

Use:

```text
Register from Local Agent
```

This is the preferred path because it keeps the worker identity consistent with the agent-managed worker.

### Step 8: Verify Machine A is healthy

Check the local agent:

```bash
curl -s http://127.0.0.1:7081/status
```

Healthy values should include:

- `"running":true`
- `"identity_match":true`
- `"multiaddr_match":true`

Check the satellite worker list:

```bash
curl -s http://127.0.0.1:7070/workers
```

Check the UI:

```text
http://localhost:3000/workers
```

Machine A should appear there with its LAN IP in `multiaddr`.

## 6. Machine B Setup

Machine B is another machine on the same LAN.

In the examples below, replace:

- `<MACHINE_A_IP>` with Machine A's LAN IP
- `<MACHINE_B_IP>` with Machine B's LAN IP

Example:

- `<MACHINE_A_IP>` -> `192.168.0.101`
- `<MACHINE_B_IP>` -> `192.168.0.115`

### Step 1: Verify Machine B can reach Machine A

From Machine B:

```bash
curl -s http://<MACHINE_A_IP>:7070/workers
```

If this fails, do not continue until Machine B can reach Machine A over the network.

### Step 2: Open two or three terminals on Machine B

You will use:

- Terminal 1 for the local agent
- Terminal 2 for the frontend
- optional Terminal 3 for verification commands

### Step 3: Start the local agent on Machine B

In Terminal 1:

```bash
cd /path/to/Dsprout/server
DSPROUT_SATELLITE_URL=http://<MACHINE_A_IP>:7070 cargo run -p dsprout-agent
```

Quick check:

```bash
curl -s http://127.0.0.1:7081/status
```

### Step 4: Configure the frontend on Machine B

Create or update `app/.env.local`:

```env
SATELLITE_URL=http://<MACHINE_A_IP>:7070
LOCAL_AGENT_URL=http://127.0.0.1:7081
```

### Step 5: Start the frontend on Machine B

In Terminal 2:

```bash
cd /path/to/Dsprout/app
npm run dev
```

Open:

```text
http://localhost:3000
```

### Step 6: Configure Machine B's worker from the UI

Open:

```text
http://localhost:3000/agent
```

Set:

```text
listen_multiaddr=/ip4/0.0.0.0/tcp/5901
advertise_multiaddr=/ip4/<MACHINE_B_IP>/tcp/5901
satellite_url=http://<MACHINE_A_IP>:7070
```

Then start the worker.

### Step 7: Register Machine B from the local agent

Open:

```text
http://localhost:3000/contributors
```

Use:

```text
Register from Local Agent
```

### Step 8: Verify Machine B is healthy

Check the local agent:

```bash
curl -s http://127.0.0.1:7081/status
```

Healthy values should include:

- `"running":true`
- `"identity_match":true`
- `"multiaddr_match":true`

### Step 9: Verify Machine B appears on Machine A

From Machine A:

```bash
curl -s http://127.0.0.1:7070/workers
```

Or from Machine A's frontend:

```text
http://localhost:3000/workers
```

Machine B should appear with:

- its own unique worker ID
- its own LAN IP in the `multiaddr`
- recent `last_seen`

## 7. Full API-Only Worker Control

If you do not want to configure the worker from the UI, you can do it through the agent API.

### Update worker config

Run this on the machine whose worker you want to control:

```bash
curl -s -X POST http://127.0.0.1:7081/config \
  -H 'content-type: application/json' \
  -d '{
    "listen_multiaddr": "/ip4/0.0.0.0/tcp/5901",
    "advertise_multiaddr": "/ip4/<THIS_MACHINE_IP>/tcp/5901",
    "satellite_url": "http://<MACHINE_A_IP>:7070",
    "restart_if_running": true
  }'
```

### Start the worker

```bash
curl -s -X POST http://127.0.0.1:7081/start \
  -H 'content-type: application/json' \
  -d '{}'
```

### Stop the worker

```bash
curl -s -X POST http://127.0.0.1:7081/stop \
  -H 'content-type: application/json' \
  -d '{}'
```

### Check storage usage

```bash
curl -s http://127.0.0.1:7081/storage
```

## 8. Recommended Startup Order

For a new session, use this order:

1. Start the satellite on Machine A
2. Start the agent on Machine A
3. Start the frontend on Machine A
4. Start and register Machine A's worker
5. Start the agent on Machine B
6. Start the frontend on Machine B
7. Start and register Machine B's worker
8. Check `/workers` on Machine A

## 9. What Healthy Looks Like

In the local agent status response:

- `running=true`
- `identity_match=true`
- `multiaddr_match=true`

In the workers list:

- each machine appears once as an active worker
- `multiaddr` contains the correct LAN IP for that machine
- `last_seen` stays fresh

In the UI:

- `/agent` shows the local worker as running
- `/workers` shows both Machine A and Machine B

## 10. Common Failures

### Problem: Machine B cannot reach the satellite

Typical causes:

- Machine A satellite is not running
- wrong Machine A IP
- machines are on different Wi-Fi networks
- firewall is blocking port `7070`

Check:

```bash
curl -s http://<MACHINE_A_IP>:7070/workers
```

### Problem: worker appears stale

Typical causes:

- worker process is not running
- wrong advertise address
- old stale worker record is still in the satellite
- agent started, but worker was never started

Check:

```bash
curl -s http://127.0.0.1:7081/status
```

### Problem: identity mismatch

Typical causes:

- worker was manually registered with the wrong ID
- an older worker identity is still stored in the satellite
- registration was done outside the local agent flow

Preferred fix:

- stop the worker
- start the worker from `/agent`
- use `Register from Local Agent` in `/contributors`

### Problem: loopback or invalid multiaddr

Bad examples:

- `/ip4/127.0.0.1/tcp/5901`
- `/ip4/0.0.0.0/tcp/5901`
- `/dns4/localhost/tcp/5901`

Good example:

- `/ip4/<THIS_MACHINE_IP>/tcp/5901`

## 11. Useful Commands

Check satellite:

```bash
curl -s http://127.0.0.1:7070/workers
```

Check local agent:

```bash
curl -s http://127.0.0.1:7081/status
```

Run frontend:

```bash
cd app
npm run dev
```

Run satellite:

```bash
cd server
cargo run -p dsprout-satellite
```

Run agent:

```bash
cd server
DSPROUT_SATELLITE_URL=http://<MACHINE_A_IP>:7070 cargo run -p dsprout-agent
```

## 12. Shutdown

If you started services manually in terminals, stop them with `Ctrl+C` in the terminals where they are running.

If you want a clean shutdown order:

1. Stop the frontend
2. Stop the local agent
3. Stop the satellite

Stopping the worker through the agent before shutting down is also fine:

```bash
curl -s -X POST http://127.0.0.1:7081/stop \
  -H 'content-type: application/json' \
  -d '{}'
```

## 13. Summary

The main rule is simple:

- Machine A hosts the satellite
- every other machine points to Machine A's LAN IP for `SATELLITE_URL`
- every worker advertises its own LAN IP
- the local agent always remains on `127.0.0.1:7081`

If you keep those four rules correct, Machine A and Machine B setup is usually straightforward.
