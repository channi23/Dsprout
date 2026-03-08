import Link from "next/link";
import { redirect } from "next/navigation";
import {
  fetchJson,
  satelliteBaseUrl,
  type LocateResp,
  type RepairApiResp,
  type SignedManifest,
  type WorkerInfo,
} from "@/lib/satellite";

type FileLookupPageProps = {
  searchParams: Promise<{
    file_id?: string;
    repair_ok?: string;
    repair_err?: string;
    target_rf?: string;
    repaired_shards?: string;
    new_replicas?: string;
  }>;
};

export const dynamic = "force-dynamic";

function parsePositiveInt(value: string | undefined, fallback: number): number {
  const parsed = Number.parseInt((value || "").trim(), 10);
  return Number.isFinite(parsed) && parsed >= 1 ? parsed : fallback;
}

async function repairAction(formData: FormData) {
  "use server";

  const fileId = String(formData.get("file_id") || "").trim();
  const replicationFactor = parsePositiveInt(String(formData.get("replication_factor") || ""), 2);
  if (!fileId) {
    redirect("/files?repair_err=file_id+is+required");
  }

  try {
    const base = satelliteBaseUrl();
    const res = await fetch(`${base}/repair`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      body: JSON.stringify({
        file_id: fileId,
        replication_factor: replicationFactor,
      }),
    });
    if (!res.ok) {
      throw new Error(await res.text());
    }
    const json = (await res.json()) as RepairApiResp;
    redirect(
      `/files?file_id=${encodeURIComponent(fileId)}&repair_ok=1&target_rf=${
        json.target_replication_factor
      }&repaired_shards=${json.repaired_shards}&new_replicas=${json.new_replicas}`,
    );
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    redirect(`/files?file_id=${encodeURIComponent(fileId)}&repair_err=${encodeURIComponent(msg)}`);
  }
}

export default async function FilesPage({ searchParams }: FileLookupPageProps) {
  const params = await searchParams;
  const fileId = (params.file_id || "").trim();
  const base = satelliteBaseUrl();
  const targetReplication = parsePositiveInt(params.target_rf, 2);

  let manifest: SignedManifest | null = null;
  let locate: LocateResp | null = null;
  let workers: WorkerInfo[] = [];
  let error: string | null = null;

  try {
    workers = await fetchJson<WorkerInfo[]>(`${base}/workers`);
    if (fileId) {
      const [manifestRes, locateRes] = await Promise.all([
        fetch(`${base}/manifest?file_id=${encodeURIComponent(fileId)}`, { cache: "no-store" }),
        fetchJson<LocateResp>(`${base}/locate?file_id=${encodeURIComponent(fileId)}`),
      ]);
      if (manifestRes.ok) {
        manifest = (await manifestRes.json()) as SignedManifest;
      }
      locate = locateRes;
    }
  } catch (err) {
    error = err instanceof Error ? err.message : String(err);
  }

  const workerById = new Map(workers.map((w) => [w.worker_id, w]));
  const grouped = new Map<string, Set<string>>();
  for (const rec of locate?.shards || []) {
    const key = `${rec.segment_index}:${rec.shard_index}`;
    if (!grouped.has(key)) grouped.set(key, new Set());
    grouped.get(key)!.add(rec.worker_id);
  }
  const replicaCounts = Array.from(grouped.values()).map((s) => s.size);
  const shardRecordCount = locate?.shards.length || 0;
  const uniqueShardCount = grouped.size;
  const minReplicas = replicaCounts.length ? Math.min(...replicaCounts) : 0;
  const maxReplicas = replicaCounts.length ? Math.max(...replicaCounts) : 0;
  const avgReplicas = replicaCounts.length
    ? replicaCounts.reduce((sum, count) => sum + count, 0) / replicaCounts.length
    : 0;
  const expectedUniqueShardCount = manifest ? manifest.manifest.segments.length * 80 : null;
  const underReplicated = uniqueShardCount > 0 && minReplicas < targetReplication;
  const degraded = !!manifest && expectedUniqueShardCount !== null && uniqueShardCount < expectedUniqueShardCount;
  const healthStatus = !manifest || uniqueShardCount === 0 ? "degraded" : underReplicated ? "under-replicated" : degraded ? "degraded" : "healthy";
  const workerShardRecords = new Map<string, number>();
  for (const rec of locate?.shards || []) {
    workerShardRecords.set(rec.worker_id, (workerShardRecords.get(rec.worker_id) || 0) + 1);
  }
  const workerPlacement = Array.from(workerShardRecords.entries()).sort((a, b) => b[1] - a[1]);

  return (
    <main className="min-h-screen p-6 md:p-10">
      <h1 className="text-2xl font-semibold">File Lookup</h1>
      <p className="mt-1 text-sm text-gray-600">Manifest + shard location detail from satellite.</p>

      <div className="mt-4 flex gap-3 text-sm">
        <Link className="underline" href="/">
          Home
        </Link>
        <Link className="underline" href="/files/upload">
          Upload
        </Link>
        <Link className="underline" href="/files/download">
          Download
        </Link>
      </div>

      <form className="mt-6 flex gap-2" method="get">
        <input
          name="file_id"
          defaultValue={fileId}
          placeholder="Enter file_id"
          className="w-full max-w-xl rounded border px-3 py-2 text-sm"
        />
        <button type="submit" className="rounded border px-4 py-2 text-sm font-medium">
          Query
        </button>
      </form>

      {error ? <p className="mt-4 text-sm text-red-700">{error}</p> : null}
      {params.repair_err ? (
        <p className="mt-4 text-sm text-red-700">{decodeURIComponent(params.repair_err)}</p>
      ) : null}
      {params.repair_ok ? (
        <p className="mt-4 text-sm text-green-700">
          Repair complete: repaired_shards={params.repaired_shards || "0"} new_replicas={params.new_replicas || "0"}
        </p>
      ) : null}

      {fileId ? (
        <section className="mt-6 space-y-2 text-sm">
          <div className="space-y-1">
            <p>
              <span className="font-semibold">file_id:</span> <span className="font-mono">{fileId}</span>
            </p>
            <p>
              <span className="font-semibold">manifest summary:</span>{" "}
              {manifest
                ? `segments=${manifest.manifest.segments.length}, original_len=${manifest.manifest.original_len}, original_hash=${manifest.manifest.original_hash_hex}`
                : "manifest not found"}
            </p>
            <p>
              <span className="font-semibold">shard record count:</span> {shardRecordCount}
            </p>
            <p>
              <span className="font-semibold">unique shard count:</span> {uniqueShardCount}
              {expectedUniqueShardCount !== null ? ` / expected ${expectedUniqueShardCount}` : ""}
            </p>
            <p>
              <span className="font-semibold">replica min/max/avg:</span> min={minReplicas}, max={maxReplicas}, avg=
              {avgReplicas.toFixed(2)}
            </p>
            <p>
              <span className="font-semibold">status:</span> {healthStatus}
            </p>
            <p>
              <span className="font-semibold">worker placement summary:</span>{" "}
              {workerPlacement.length
                ? workerPlacement.map(([wid, count]) => `${wid}:${count}`).join(", ")
                : "no placements"}
            </p>
          </div>

          <form action={repairAction} className="mt-4 flex flex-wrap items-end gap-2 border rounded p-3">
            <input type="hidden" name="file_id" value={fileId} />
            <label className="text-sm">
              target replication
              <input
                name="replication_factor"
                type="number"
                min="1"
                defaultValue={targetReplication}
                className="mt-1 block w-36 rounded border px-2 py-1"
              />
            </label>
            <button type="submit" className="rounded border px-4 py-2 text-sm font-medium">
              Run Repair
            </button>
          </form>

          {locate?.shards?.length ? (
            <div className="mt-4 overflow-auto border rounded">
              <table className="min-w-full text-sm">
                <thead>
                  <tr className="bg-gray-100 text-left">
                    <th className="px-3 py-2">segment</th>
                    <th className="px-3 py-2">shard</th>
                    <th className="px-3 py-2">worker</th>
                    <th className="px-3 py-2">enabled</th>
                    <th className="px-3 py-2">owner</th>
                  </tr>
                </thead>
                <tbody>
                  {locate.shards.map((rec, idx) => {
                    const w = workerById.get(rec.worker_id);
                    return (
                      <tr key={`${rec.segment_index}-${rec.shard_index}-${rec.worker_id}-${idx}`} className="border-t">
                        <td className="px-3 py-2">{rec.segment_index}</td>
                        <td className="px-3 py-2">{rec.shard_index}</td>
                        <td className="px-3 py-2 font-mono">{rec.worker_id}</td>
                        <td className="px-3 py-2">{w ? (w.enabled ? "yes" : "no") : "?"}</td>
                        <td className="px-3 py-2">{w?.owner_label || ""}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          ) : null}
        </section>
      ) : null}
    </main>
  );
}
