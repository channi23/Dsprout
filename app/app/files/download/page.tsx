import Link from "next/link";
import { redirect } from "next/navigation";
import { actionableFetchError, satelliteBaseUrl, type DownloadApiResp } from "@/lib/satellite";

type DownloadPageProps = {
  searchParams: Promise<{
    ok?: string;
    err?: string;
    file_id?: string;
    original_hash?: string;
    restored_hash?: string;
    equal?: string;
    bytes?: string;
    data?: string;
  }>;
};

export const dynamic = "force-dynamic";

async function downloadAction(formData: FormData) {
  "use server";
  const fileId = String(formData.get("file_id") || "").trim();
  if (!fileId) {
    redirect("/files/download?err=file_id+is+required");
  }

  let json: DownloadApiResp;
  try {
    const base = satelliteBaseUrl();
    const res = await fetch(`${base}/download`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      body: JSON.stringify({ file_id: fileId }),
    });
    if (!res.ok) {
      const body = await res.text().catch(() => "");
      throw new Error(`${res.status} ${res.statusText}${body ? ` - ${body}` : ""}`);
    }
    json = (await res.json()) as DownloadApiResp;
  } catch (err) {
    const msg = actionableFetchError(err, `${satelliteBaseUrl()}/download`, "Download request").message;
    redirect(`/files/download?err=${encodeURIComponent(msg)}`);
  }

  redirect(
    `/files/download?ok=1&file_id=${encodeURIComponent(json.file_id)}&original_hash=${encodeURIComponent(
      json.original_hash,
    )}&restored_hash=${encodeURIComponent(json.restored_hash)}&equal=${json.equal ? "1" : "0"}&bytes=${
      json.bytes
    }&data=${encodeURIComponent(json.file_bytes_base64)}`,
  );
}

export default async function DownloadPage({ searchParams }: DownloadPageProps) {
  const params = await searchParams;
  const ok = !!params.ok;
  const equal = params.equal === "1";

  return (
    <main className="min-h-screen p-6 md:p-10">
      <h1 className="text-2xl font-semibold">Download File</h1>
      <p className="mt-1 text-sm text-gray-600">HTTP-backed download through satellite `/download`.</p>

      <div className="mt-4 flex gap-3 text-sm">
        <Link className="underline" href="/">
          Home
        </Link>
        <Link className="underline" href="/files">
          File Lookup
        </Link>
        <Link className="underline" href="/files/upload">
          Upload
        </Link>
      </div>

      {ok ? null : params.err ? <p className="mt-4 text-sm text-red-700">{decodeURIComponent(params.err)}</p> : null}

      {ok ? (
        <div className="mt-4 space-y-1 text-sm">
          <p className="text-green-700">Download successful</p>
          <p>file_id: {params.file_id}</p>
          <p>original_hash: {params.original_hash}</p>
          <p>restored_hash: {params.restored_hash}</p>
          <p>equal: {equal ? "true" : "false"}</p>
          <p>bytes: {params.bytes}</p>
          {params.data ? (
            <a
              className="inline-block mt-2 rounded border px-3 py-2 font-medium"
              href={`data:application/octet-stream;base64,${params.data}`}
              download={params.file_id || "download.bin"}
            >
              Save Downloaded File
            </a>
          ) : null}
        </div>
      ) : null}

      <form action={downloadAction} className="mt-6 grid max-w-2xl gap-3 text-sm">
        <label>
          file_id
          <input name="file_id" required className="mt-1 block w-full rounded border px-3 py-2 font-mono" />
        </label>
        <button type="submit" className="mt-2 w-fit rounded border px-4 py-2 font-medium">
          Download
        </button>
      </form>
    </main>
  );
}
