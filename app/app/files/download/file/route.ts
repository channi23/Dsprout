import { satelliteBaseUrl, type DownloadApiResp } from "@/lib/satellite";

export const dynamic = "force-dynamic";

export async function GET(req: Request) {
  const { searchParams } = new URL(req.url);
  const fileId = searchParams.get("file_id")?.trim() || "";

  if (!fileId) {
    return new Response("file_id is required", { status: 400 });
  }

  const res = await fetch(`${satelliteBaseUrl()}/download`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    cache: "no-store",
    body: JSON.stringify({ file_id: fileId }),
  });

  if (!res.ok) {
    const body = await res.text().catch(() => "");
    return new Response(body || "download failed", { status: res.status });
  }

  const json = (await res.json()) as DownloadApiResp;
  const bytes = Buffer.from(json.file_bytes_base64, "base64");

  return new Response(bytes, {
    status: 200,
    headers: {
      "content-type": "application/octet-stream",
      "content-length": String(bytes.length),
      "content-disposition": `attachment; filename="${json.file_id || "download.bin"}"`,
      "cache-control": "no-store",
    },
  });
}
