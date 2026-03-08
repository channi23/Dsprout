import Link from "next/link";
import { fetchJson, formatBytes, satelliteBaseUrl, type WorkerInfo } from "@/lib/satellite";

const HEALTHY_MAX_AGE_MS = 30_000;

export const dynamic = "force-dynamic";

function fmtAge(ms: number): string {
  const secs = Math.max(0, Math.floor(ms / 1000));
  return `${secs}s`;
}

export default async function WorkersPage() {
  const base = satelliteBaseUrl();
  const workers = await fetchJson<WorkerInfo[]>(`${base}/workers`);
  const newestLastSeen = workers.reduce((max, w) => Math.max(max, Number(w.last_seen) || 0), 0);

  return (
    <main className="min-h-screen p-6 md:p-10">
      <h1 className="text-2xl font-semibold">Workers</h1>
      <p className="mt-1 text-sm text-gray-600">{workers.length} workers</p>
      <p className="mt-1 text-xs text-gray-500">Health threshold: {HEALTHY_MAX_AGE_MS / 1000}s lag</p>

      <div className="mt-4 flex gap-3 text-sm">
        <Link className="underline" href="/">
          Home
        </Link>
        <Link className="underline" href="/contributors">
          Contributor Registration
        </Link>
      </div>

      <div className="mt-5 overflow-auto border rounded">
        <table className="min-w-full text-sm">
          <thead>
            <tr className="bg-gray-100 text-left">
              <th className="px-3 py-2">worker_id</th>
              <th className="px-3 py-2">device</th>
              <th className="px-3 py-2">owner</th>
              <th className="px-3 py-2">capacity</th>
              <th className="px-3 py-2">used</th>
              <th className="px-3 py-2">enabled</th>
              <th className="px-3 py-2">last_seen_lag</th>
              <th className="px-3 py-2">health</th>
            </tr>
          </thead>
          <tbody>
            {workers.map((w) => {
              const lag = Math.max(0, newestLastSeen - Number(w.last_seen));
              const healthy = lag <= HEALTHY_MAX_AGE_MS;
              return (
                <tr key={w.worker_id} className="border-t">
                  <td className="px-3 py-2 font-mono">
                    <Link className="underline" href={`/workers/${encodeURIComponent(w.worker_id)}`}>
                      {w.worker_id}
                    </Link>
                  </td>
                  <td className="px-3 py-2">{w.device_name}</td>
                  <td className="px-3 py-2">{w.owner_label}</td>
                  <td className="px-3 py-2">{formatBytes(Number(w.capacity_limit_bytes) || 0)}</td>
                  <td className="px-3 py-2">{formatBytes(Number(w.used_bytes) || 0)}</td>
                  <td className="px-3 py-2">{w.enabled ? "yes" : "no"}</td>
                  <td className="px-3 py-2">{fmtAge(lag)}</td>
                  <td className={`px-3 py-2 ${healthy ? "text-green-700" : "text-red-700"}`}>
                    {healthy ? "healthy" : "stale"}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </main>
  );
}
