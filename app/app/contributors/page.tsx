import Link from "next/link";
import { redirect } from "next/navigation";
import {
  actionableFetchError,
  fetchJson,
  localAgentBaseUrl,
  postJson,
  satelliteBaseUrl,
  type RegisterWorkerReq,
  type WorkerAgentStatusResp,
  type WorkerAgentStorageResp,
} from "@/lib/satellite";

type ContributorsPageProps = {
  searchParams: Promise<{ ok?: string; err?: string }>;
};

export const dynamic = "force-dynamic";

function isInvalidAdvertiseMultiaddr(multiaddr: string): string | null {
  if (multiaddr.includes("/ip4/0.0.0.0/")) {
    return "multiaddr cannot advertise 0.0.0.0; use a reachable LAN IP like /ip4/192.168.x.x/tcp/5901";
  }
  if (multiaddr.includes("/ip4/127.")) {
    return "multiaddr cannot advertise 127.x.x.x for shared-LAN use; use your machine LAN IP";
  }
  if (multiaddr.toLowerCase().includes("localhost")) {
    return "multiaddr cannot advertise localhost for shared-LAN use; use your machine LAN IP";
  }
  return null;
}

async function registerContributorWorker(formData: FormData) {
  "use server";

  const base = satelliteBaseUrl();
  const enabledRaw = String(formData.get("enabled") || "true");
  const allowMismatch = String(formData.get("allow_agent_mismatch") || "") === "on";
  const payload: RegisterWorkerReq = {
    worker_id: String(formData.get("worker_id") || "").trim(),
    multiaddr: String(formData.get("multiaddr") || "").trim(),
    device_name: String(formData.get("device_name") || "").trim(),
    owner_label: String(formData.get("owner_label") || "").trim(),
    capacity_limit_bytes: Number(formData.get("capacity_limit_bytes") || 0),
    used_bytes: Number(formData.get("used_bytes") || 0),
    enabled: enabledRaw === "true" || enabledRaw === "1",
  };

  if (!payload.worker_id) {
    redirect("/contributors?err=worker_id+is+required");
  }
  if (!payload.multiaddr) {
    redirect("/contributors?err=multiaddr+is+required");
  }
  const invalidMultiaddr = isInvalidAdvertiseMultiaddr(payload.multiaddr);
  if (invalidMultiaddr) {
    redirect(`/contributors?err=${encodeURIComponent(invalidMultiaddr)}`);
  }
  if (!Number.isFinite(payload.capacity_limit_bytes) || payload.capacity_limit_bytes < 0) {
    redirect("/contributors?err=capacity_limit_bytes+must+be+>=+0");
  }
  if (!Number.isFinite(payload.used_bytes) || payload.used_bytes < 0) {
    redirect("/contributors?err=used_bytes+must+be+>=+0");
  }

  const localAgent = localAgentBaseUrl();
  try {
    const status = await fetchJson<WorkerAgentStatusResp>(`${localAgent}/status`);
    if (
      !allowMismatch &&
      status.config.worker_id &&
      status.config.worker_id !== payload.worker_id
    ) {
      redirect(
        `/contributors?err=${encodeURIComponent(
          `worker_id mismatch: form=${payload.worker_id} local_agent=${status.config.worker_id}. Use local agent identity or check 'Allow manual mismatch'.`
        )}`
      );
    }
  } catch {
    // Manual registration is still allowed if local agent is not available.
  }

  try {
    await postJson(`${base}/register_worker`, payload);
  } catch (err) {
    const msg = actionableFetchError(err, `${base}/register_worker`, "Worker registration").message;
    redirect(`/contributors?err=${encodeURIComponent(msg)}`);
  }
  redirect("/contributors?ok=1");
}

async function registerFromLocalAgent() {
  "use server";

  const satellite = satelliteBaseUrl();
  const agent = localAgentBaseUrl();
  try {
    const [status, storage] = await Promise.all([
      fetchJson<WorkerAgentStatusResp>(`${agent}/status`),
      fetchJson<WorkerAgentStorageResp>(`${agent}/storage`),
    ]);
    const invalidMultiaddr = isInvalidAdvertiseMultiaddr(status.config.advertise_multiaddr);
    if (invalidMultiaddr) {
      throw new Error(
        `Local agent advertise_multiaddr is invalid: ${status.config.advertise_multiaddr}. ${invalidMultiaddr}`
      );
    }
    await postJson(`${satellite}/register_worker`, {
      worker_id: status.config.worker_id,
      multiaddr: status.config.advertise_multiaddr,
      device_name: status.config.device_name,
      owner_label: status.config.owner_label,
      capacity_limit_bytes: status.config.capacity_limit_bytes,
      used_bytes: storage.used_bytes,
      enabled: status.config.enabled,
    } satisfies RegisterWorkerReq);
  } catch (err) {
    const msg = actionableFetchError(
      err,
      `${satellite}/register_worker`,
      "Register from local agent"
    ).message;
    redirect(`/contributors?err=${encodeURIComponent(msg)}`);
  }
  redirect("/contributors?ok=1");
}

export default async function ContributorsPage({ searchParams }: ContributorsPageProps) {
  const params = await searchParams;
  const agentBase = localAgentBaseUrl();
  let localStatus: WorkerAgentStatusResp | null = null;
  let localStorage: WorkerAgentStorageResp | null = null;
  let localErr: string | null = null;
  try {
    [localStatus, localStorage] = await Promise.all([
      fetchJson<WorkerAgentStatusResp>(`${agentBase}/status`),
      fetchJson<WorkerAgentStorageResp>(`${agentBase}/storage`),
    ]);
  } catch (err) {
    localErr = actionableFetchError(err, `${agentBase}/status`, "Local agent lookup").message;
  }

  return (
    <main className="min-h-screen p-6 md:p-10">
      <h1 className="text-2xl font-semibold">Contributor Registration</h1>
      <p className="mt-1 text-sm text-gray-600">Register or update worker metadata in satellite.</p>
      <p className="mt-1 text-xs text-gray-500">
        Prefer &quot;Register from Local Agent&quot; to avoid worker identity mismatches.
      </p>

      <div className="mt-4 flex gap-3 text-sm">
        <Link className="underline" href="/">
          Home
        </Link>
        <Link className="underline" href="/workers">
          Worker List
        </Link>
        <Link className="underline" href="/agent">
          Worker Control Panel
        </Link>
      </div>

      {params.ok ? <p className="mt-4 text-sm text-green-700">Worker metadata registered.</p> : null}
      {params.err ? <p className="mt-4 text-sm text-red-700">{decodeURIComponent(params.err)}</p> : null}
      {localErr ? <p className="mt-4 text-sm text-amber-700">{localErr}</p> : null}

      {localStatus ? (
        <section className="mt-4 rounded border p-3 text-sm">
          <p>
            <span className="font-semibold">local agent worker_id:</span>{" "}
            <span className="font-mono">{localStatus.config.worker_id}</span>
          </p>
          <p>
            <span className="font-semibold">advertise_multiaddr:</span>{" "}
            <span className="font-mono">{localStatus.config.advertise_multiaddr}</span>
          </p>
          <p>
            <span className="font-semibold">used_bytes:</span> {localStorage?.used_bytes ?? 0}
          </p>
          <form action={registerFromLocalAgent} className="mt-3">
            <button type="submit" className="rounded border px-4 py-2 font-medium">
              Register from Local Agent
            </button>
          </form>
        </section>
      ) : null}

      <form action={registerContributorWorker} className="mt-6 grid max-w-2xl gap-3 text-sm">
        <label>
          worker_id
          <input name="worker_id" required className="mt-1 block w-full rounded border px-3 py-2 font-mono" />
        </label>
        <label>
          multiaddr
          <input
            name="multiaddr"
            required
            placeholder={localStatus?.config.advertise_multiaddr || "/ip4/192.168.1.50/tcp/5901"}
            className="mt-1 block w-full rounded border px-3 py-2 font-mono"
          />
        </label>
        <label>
          device_name
          <input name="device_name" required className="mt-1 block w-full rounded border px-3 py-2" />
        </label>
        <label>
          owner_label
          <input name="owner_label" required className="mt-1 block w-full rounded border px-3 py-2" />
        </label>
        <label>
          capacity_limit_bytes
          <input
            name="capacity_limit_bytes"
            type="number"
            min="0"
            defaultValue="10737418240"
            required
            className="mt-1 block w-full rounded border px-3 py-2"
          />
        </label>
        <label>
          used_bytes
          <input
            name="used_bytes"
            type="number"
            min="0"
            defaultValue={String(localStorage?.used_bytes ?? 0)}
            required
            className="mt-1 block w-full rounded border px-3 py-2"
          />
        </label>
        <label>
          enabled
          <select name="enabled" defaultValue="true" className="mt-1 block w-full rounded border px-3 py-2">
            <option value="true">true</option>
            <option value="false">false</option>
          </select>
        </label>
        <label className="inline-flex items-center gap-2">
          <input name="allow_agent_mismatch" type="checkbox" />
          Allow manual mismatch vs local agent identity
        </label>

        <button type="submit" className="mt-2 w-fit rounded border px-4 py-2 font-medium">
          Register Worker Metadata
        </button>
      </form>
    </main>
  );
}
