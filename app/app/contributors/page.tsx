import Link from "next/link";
import { redirect } from "next/navigation";
import { postJson, satelliteBaseUrl, type RegisterWorkerReq } from "@/lib/satellite";

type ContributorsPageProps = {
  searchParams: Promise<{ ok?: string; err?: string }>;
};

export const dynamic = "force-dynamic";

async function registerContributorWorker(formData: FormData) {
  "use server";

  const base = satelliteBaseUrl();
  const enabledRaw = String(formData.get("enabled") || "true");
  const payload: RegisterWorkerReq = {
    worker_id: String(formData.get("worker_id") || "").trim(),
    multiaddr: String(formData.get("multiaddr") || "").trim(),
    device_name: String(formData.get("device_name") || "").trim(),
    owner_label: String(formData.get("owner_label") || "").trim(),
    capacity_limit_bytes: Number(formData.get("capacity_limit_bytes") || 0),
    used_bytes: Number(formData.get("used_bytes") || 0),
    enabled: enabledRaw === "true" || enabledRaw === "1",
  };

  try {
    await postJson(`${base}/register_worker`, payload);
    redirect("/contributors?ok=1");
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    redirect(`/contributors?err=${encodeURIComponent(msg)}`);
  }
}

export default async function ContributorsPage({ searchParams }: ContributorsPageProps) {
  const params = await searchParams;

  return (
    <main className="min-h-screen p-6 md:p-10">
      <h1 className="text-2xl font-semibold">Contributor Registration</h1>
      <p className="mt-1 text-sm text-gray-600">Register or update worker metadata in satellite.</p>

      <div className="mt-4 flex gap-3 text-sm">
        <Link className="underline" href="/">
          Home
        </Link>
        <Link className="underline" href="/workers">
          Worker List
        </Link>
      </div>

      {params.ok ? <p className="mt-4 text-sm text-green-700">Worker metadata registered.</p> : null}
      {params.err ? <p className="mt-4 text-sm text-red-700">{decodeURIComponent(params.err)}</p> : null}

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
            placeholder="/ip4/127.0.0.1/tcp/5701"
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
            defaultValue="0"
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

        <button type="submit" className="mt-2 w-fit rounded border px-4 py-2 font-medium">
          Register Worker Metadata
        </button>
      </form>
    </main>
  );
}
