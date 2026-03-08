import Link from "next/link";
import {
  fetchJson,
  satelliteBaseUrl,
  type LocateResp,
  type SignedManifest,
  type WorkerInfo,
} from "@/lib/satellite";

type FileLookupPageProps = {
  searchParams: Promise<{ file_id?: string }>;
};

export const dynamic = "force-dynamic";

export default async function FilesPage({ searchParams }: FileLookupPageProps) {
  const params = await searchParams;
  const fileId = (params.file_id || "").trim();
  const base = satelliteBaseUrl();

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
  const minReplicas = replicaCounts.length ? Math.min(...replicaCounts) : 0;
  const maxReplicas = replicaCounts.length ? Math.max(...replicaCounts) : 0;

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

      {fileId ? (
        <section className="mt-6 space-y-2 text-sm">
          <p>
            <span className="font-semibold">file_id:</span> <span className="font-mono">{fileId}</span>
          </p>
          <p>
            <span className="font-semibold">segment count:</span>{" "}
            {manifest ? manifest.manifest.segments.length : "manifest not found"}
          </p>
          <p>
            <span className="font-semibold">shard records:</span> {locate?.shards.length || 0}
          </p>
          <p>
            <span className="font-semibold">replica counts:</span> min={minReplicas}, max={maxReplicas}
          </p>

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
