import Link from "next/link";
import { redirect } from "next/navigation";
import { satelliteBaseUrl, type UploadApiReq, type UploadApiResp } from "@/lib/satellite";

type UploadPageProps = {
  searchParams: Promise<{ ok?: string; err?: string; file_id?: string; hash?: string; bytes?: string }>;
};

export const dynamic = "force-dynamic";

async function uploadAction(formData: FormData) {
  "use server";

  const file = formData.get("file") as File | null;
  if (!file) {
    redirect("/files/upload?err=missing+file");
  }

  const bytes = Buffer.from(await file.arrayBuffer());
  const payload: UploadApiReq = {
    file_bytes_base64: bytes.toString("base64"),
    file_id: String(formData.get("file_id") || "").trim() || undefined,
    replication_factor: Number(formData.get("replication_factor") || 2),
  };

  try {
    const base = satelliteBaseUrl();
    const res = await fetch(`${base}/upload`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload),
      cache: "no-store",
    });
    if (!res.ok) {
      throw new Error(await res.text());
    }
    const json = (await res.json()) as UploadApiResp;
    redirect(
      `/files/upload?ok=1&file_id=${encodeURIComponent(json.file_id)}&hash=${encodeURIComponent(
        json.input_hash,
      )}&bytes=${json.bytes}`,
    );
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    redirect(`/files/upload?err=${encodeURIComponent(msg)}`);
  }
}

export default async function UploadPage({ searchParams }: UploadPageProps) {
  const params = await searchParams;

  return (
    <main className="min-h-screen p-6 md:p-10">
      <h1 className="text-2xl font-semibold">Upload File</h1>
      <p className="mt-1 text-sm text-gray-600">HTTP-backed upload through satellite `/upload`.</p>

      <div className="mt-4 flex gap-3 text-sm">
        <Link className="underline" href="/">
          Home
        </Link>
        <Link className="underline" href="/files">
          File Lookup
        </Link>
        <Link className="underline" href="/files/download">
          Download
        </Link>
      </div>

      {params.ok ? (
        <p className="mt-4 text-sm text-green-700">
          Upload successful: file_id={params.file_id} hash={params.hash} bytes={params.bytes}
        </p>
      ) : null}
      {params.err ? <p className="mt-4 text-sm text-red-700">{decodeURIComponent(params.err)}</p> : null}

      <form action={uploadAction} className="mt-6 grid max-w-2xl gap-3 text-sm">
        <label>
          File
          <input name="file" type="file" required className="mt-1 block w-full rounded border px-3 py-2" />
        </label>
        <label>
          Optional file_id
          <input name="file_id" className="mt-1 block w-full rounded border px-3 py-2 font-mono" />
        </label>
        <label>
          replication_factor
          <input
            name="replication_factor"
            type="number"
            min="1"
            defaultValue="2"
            className="mt-1 block w-full rounded border px-3 py-2"
          />
        </label>
        <button type="submit" className="mt-2 w-fit rounded border px-4 py-2 font-medium">
          Upload
        </button>
      </form>
    </main>
  );
}
