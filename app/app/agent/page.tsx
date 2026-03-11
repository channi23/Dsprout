import Link from "next/link";
import { redirect } from "next/navigation";
import {
  actionableFetchError,
  fetchJson,
  formatBytes,
  localAgentBaseUrl,
  type WorkerAgentStatusResp,
  type WorkerAgentStorageResp,
} from "@/lib/satellite";

type AgentPageProps = {
  searchParams: Promise<{ ok?: string; err?: string; msg?: string }>;
};

export const dynamic = "force-dynamic";

async function postAgent(path: string, payload?: unknown) {
  const base = localAgentBaseUrl();
  const target = `${base}${path}`;
  let res: Response;
  try {
    res = await fetch(target, {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      body: payload === undefined ? "{}" : JSON.stringify(payload),
    });
  } catch (err) {
    throw actionableFetchError(err, target, "Local agent request");
  }
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(
      `Local agent request failed: ${res.status} ${res.statusText}${body ? ` - ${body}` : ""}. target=${target}`
    );
  }
}

async function startWorkerAction() {
  "use server";
  try {
    await postAgent("/start");
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    redirect(`/agent?err=${encodeURIComponent(msg)}`);
  }
  redirect("/agent?ok=1&msg=worker+started");
}

async function stopWorkerAction() {
  "use server";
  try {
    await postAgent("/stop");
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    redirect(`/agent?err=${encodeURIComponent(msg)}`);
  }
  redirect("/agent?ok=1&msg=worker+stopped");
}

async function updateConfigAction(formData: FormData) {
  "use server";

  const deviceName = String(formData.get("device_name") || "").trim();
  const listenMultiaddr = String(formData.get("listen_multiaddr") || "").trim();
  const advertiseMultiaddr = String(formData.get("advertise_multiaddr") || "").trim();
  const capacityRaw = String(formData.get("capacity_limit_bytes") || "").trim();
  const capacity = Number.parseInt(capacityRaw, 10);

  if (!deviceName) {
    redirect("/agent?err=device_name+is+required");
  }
  if (!listenMultiaddr) {
    redirect("/agent?err=listen_multiaddr+is+required");
  }
  if (!advertiseMultiaddr) {
    redirect("/agent?err=advertise_multiaddr+is+required");
  }
  if (!Number.isFinite(capacity) || capacity < 0) {
    redirect("/agent?err=capacity_limit_bytes+must+be+>=+0");
  }

  const restartIfRunning = String(formData.get("restart_if_running") || "") === "on";

  try {
    await postAgent("/config", {
      device_name: deviceName,
      listen_multiaddr: listenMultiaddr,
      advertise_multiaddr: advertiseMultiaddr,
      capacity_limit_bytes: capacity,
      restart_if_running: restartIfRunning,
    });
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    redirect(`/agent?err=${encodeURIComponent(msg)}`);
  }
  redirect("/agent?ok=1&msg=config+updated");
}

export default async function AgentPage({ searchParams }: AgentPageProps) {
  const params = await searchParams;

  let status: WorkerAgentStatusResp | null = null;
  let storage: WorkerAgentStorageResp | null = null;
  let loadError: string | null = null;

  try {
    const base = localAgentBaseUrl();
    [status, storage] = await Promise.all([
      fetchJson<WorkerAgentStatusResp>(`${base}/status`),
      fetchJson<WorkerAgentStorageResp>(`${base}/storage`),
    ]);
  } catch (err) {
    loadError = actionableFetchError(err, `${localAgentBaseUrl()}/status`, "Load agent status").message;
  }

  return (
    <main className="min-h-screen p-6 md:p-10">
      <h1 className="text-2xl font-semibold">Worker Control Panel</h1>
      <p className="mt-1 text-sm text-gray-600">Local worker agent: {localAgentBaseUrl()}</p>

      <div className="mt-4 flex gap-3 text-sm">
        <Link className="underline" href="/">
          Home
        </Link>
        <Link className="underline" href="/workers">
          Worker List
        </Link>
        <Link className="underline" href="/contributors">
          Contributor Registration
        </Link>
      </div>

      {params.ok ? <p className="mt-4 text-sm text-green-700">{decodeURIComponent(params.msg || "ok")}</p> : null}
      {params.err ? <p className="mt-4 text-sm text-red-700">{decodeURIComponent(params.err)}</p> : null}
      {loadError ? <p className="mt-4 text-sm text-red-700">{loadError}</p> : null}

      {status ? (
        <section className="mt-6 space-y-2 text-sm">
          <p>
            <span className="font-semibold">worker status:</span>{" "}
            <span className={status.running ? "text-green-700" : "text-gray-700"}>
              {status.running ? "running" : "stopped"}
            </span>
          </p>
          <p>
            <span className="font-semibold">pid:</span> {status.pid ?? "-"}
          </p>
          <p>
            <span className="font-semibold">local worker_id:</span>{" "}
            <span className="font-mono">{status.config.worker_id}</span>
          </p>
          <p>
            <span className="font-semibold">device_name:</span> {status.config.device_name}
          </p>
          <p>
            <span className="font-semibold">capacity_limit:</span> {formatBytes(status.config.capacity_limit_bytes)} ({status.config.capacity_limit_bytes} bytes)
          </p>
          <p>
            <span className="font-semibold">profile:</span> {status.config.profile}
          </p>
          <p>
            <span className="font-semibold">listen:</span> <span className="font-mono">{status.config.listen_multiaddr}</span>
          </p>
          <p>
            <span className="font-semibold">advertise:</span>{" "}
            <span className="font-mono">{status.config.advertise_multiaddr}</span>
          </p>
          <p>
            <span className="font-semibold">satellite_url:</span> <span className="font-mono">{status.config.satellite_url}</span>
          </p>
          <p>
            <span className="font-semibold">satellite worker_id:</span>{" "}
            <span className="font-mono">{status.satellite?.worker_id || "-"}</span>
          </p>
          <p>
            <span className="font-semibold">identity match:</span>{" "}
            {status.identity_match === null ? (
              "-"
            ) : status.identity_match ? (
              <span className="text-green-700">yes</span>
            ) : (
              <span className="text-red-700">no (local worker_id differs from satellite record)</span>
            )}
          </p>
          <p>
            <span className="font-semibold">satellite multiaddr:</span>{" "}
            <span className="font-mono">{status.satellite?.multiaddr || "-"}</span>
          </p>
          {status.satellite && status.satellite.multiaddr !== status.config.advertise_multiaddr ? (
            <p className="text-red-700">
              satellite multiaddr mismatch: local advertise is{" "}
              <span className="font-mono">{status.config.advertise_multiaddr}</span>
            </p>
          ) : null}
          {status.config.advertise_multiaddr.includes("/ip4/127.") ||
          status.config.advertise_multiaddr.includes("/ip4/0.0.0.0/") ? (
            <p className="text-red-700">
              advertise_multiaddr is loopback/unspecified. For shared LAN workers, advertise your machine LAN IP.
            </p>
          ) : null}
          <p>
            <span className="font-semibold">last_exit_code:</span> {status.last_exit_code ?? "-"}
          </p>
          <p>
            <span className="font-semibold">last_error:</span> {status.last_error || "-"}
          </p>
        </section>
      ) : null}

      {storage ? (
        <section className="mt-6 space-y-1 text-sm">
          <p>
            <span className="font-semibold">used bytes:</span> {formatBytes(storage.used_bytes)} ({storage.used_bytes} bytes)
          </p>
          <p>
            <span className="font-semibold">hosted shard count:</span> {storage.hosted_shards}
          </p>
        </section>
      ) : null}

      <section className="mt-6 flex gap-2 text-sm">
        <form action={startWorkerAction}>
          <button type="submit" className="rounded border px-4 py-2 font-medium">
            Start Worker
          </button>
        </form>
        <form action={stopWorkerAction}>
          <button type="submit" className="rounded border px-4 py-2 font-medium">
            Stop Worker
          </button>
        </form>
      </section>

      {status ? (
        <form action={updateConfigAction} className="mt-6 grid max-w-2xl gap-3 text-sm">
          <p className="text-xs text-gray-500">
            LAN tip: use listen <span className="font-mono">/ip4/0.0.0.0/tcp/5901</span> and advertise your LAN IP
            multiaddr.
          </p>
          <label>
            device_name
            <input
              name="device_name"
              required
              defaultValue={status.config.device_name}
              className="mt-1 block w-full rounded border px-3 py-2"
            />
          </label>
          <label>
            capacity_limit_bytes
            <input
              name="capacity_limit_bytes"
              type="number"
              min="0"
              required
              defaultValue={String(status.config.capacity_limit_bytes)}
              className="mt-1 block w-full rounded border px-3 py-2"
            />
          </label>
          <label>
            listen_multiaddr
            <input
              name="listen_multiaddr"
              required
              defaultValue={status.config.listen_multiaddr}
              className="mt-1 block w-full rounded border px-3 py-2 font-mono"
            />
          </label>
          <label>
            advertise_multiaddr
            <input
              name="advertise_multiaddr"
              required
              defaultValue={status.config.advertise_multiaddr}
              className="mt-1 block w-full rounded border px-3 py-2 font-mono"
            />
          </label>
          <label className="inline-flex items-center gap-2">
            <input name="restart_if_running" type="checkbox" defaultChecked />
            Restart worker immediately if running
          </label>
          <button type="submit" className="w-fit rounded border px-4 py-2 font-medium">
            Update Config
          </button>
        </form>
      ) : null}
    </main>
  );
}
