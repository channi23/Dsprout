import Link from "next/link";
import { fetchJson, formatBytes, satelliteBaseUrl, type WorkerInfo } from "@/lib/satellite";

const HEALTHY_MAX_AGE_MS = 30_000;

type WorkerDetailPageProps = {
  params: Promise<{ worker_id: string }>;
};

export const dynamic = "force-dynamic";

export default async function WorkerDetailPage({ params }: WorkerDetailPageProps) {
  const { worker_id } = await params;
  const base = satelliteBaseUrl();
  const worker = await fetchJson<WorkerInfo>(
    `${base}/worker?worker_id=${encodeURIComponent(worker_id)}`,
  );

  const workers = await fetchJson<WorkerInfo[]>(`${base}/workers`);
  const newestLastSeen = workers.reduce((max, w) => Math.max(max, Number(w.last_seen) || 0), 0);
  const lagMs = Math.max(0, newestLastSeen - Number(worker.last_seen));
  const healthy = lagMs <= HEALTHY_MAX_AGE_MS;

  return (
    <main className="min-h-screen p-6 md:p-10">
      <h1 className="text-2xl font-semibold">Worker Detail</h1>
      <p className="mt-1 text-sm font-mono">{worker.worker_id}</p>

      <div className="mt-4 flex gap-3 text-sm">
        <Link className="underline" href="/workers">
          Back to Workers
        </Link>
        <Link className="underline" href="/contributors">
          Contributor Registration
        </Link>
      </div>

      <div className="mt-6 space-y-2 text-sm">
        <div>
          <span className="font-semibold">multiaddr:</span> <span className="font-mono">{worker.multiaddr}</span>
        </div>
        <div>
          <span className="font-semibold">device_name:</span> {worker.device_name}
        </div>
        <div>
          <span className="font-semibold">owner_label:</span> {worker.owner_label}
        </div>
        <div>
          <span className="font-semibold">capacity_limit_bytes:</span> {formatBytes(Number(worker.capacity_limit_bytes) || 0)}
        </div>
        <div>
          <span className="font-semibold">used_bytes:</span> {formatBytes(Number(worker.used_bytes) || 0)}
        </div>
        <div>
          <span className="font-semibold">enabled:</span> {worker.enabled ? "yes" : "no"}
        </div>
        <div>
          <span className="font-semibold">last_seen_lag:</span> {Math.floor(lagMs / 1000)}s
        </div>
        <div>
          <span className="font-semibold">health:</span>{" "}
          <span className={healthy ? "text-green-700" : "text-red-700"}>
            {healthy ? "healthy" : "stale"}
          </span>
        </div>
      </div>
    </main>
  );
}
