export type WorkerInfo = {
  worker_id: string;
  multiaddr: string;
  device_name: string;
  owner_label: string;
  capacity_limit_bytes: number;
  used_bytes: number;
  enabled: boolean;
  last_seen: number;
};

export type RegisterWorkerReq = {
  worker_id: string;
  multiaddr: string;
  device_name: string;
  owner_label: string;
  capacity_limit_bytes: number;
  used_bytes: number;
  enabled: boolean;
};

export type ManifestSegment = {
  segment_index: number;
  plaintext_len: number;
  ciphertext_len: number;
  nonce: number[];
};

export type SignedManifest = {
  manifest: {
    file_id: string;
    original_len: number;
    original_hash_hex: string;
    segments: ManifestSegment[];
  };
  uploader_peer_id: string;
  uploader_public_key_protobuf: number[];
  signature: number[];
};

export type ShardRecord = {
  worker_id: string;
  worker_multiaddr: string;
  file_id: string;
  segment_index: number;
  shard_index: number;
  shard_hash_hex: string;
};

export type LocateResp = {
  file_id: string;
  shards: ShardRecord[];
};

export type UploadApiReq = {
  file_bytes_base64: string;
  file_id?: string;
  replication_factor?: number;
};

export type UploadApiResp = {
  status: string;
  file_id: string;
  input_hash: string;
  bytes: number;
  replication_factor: number;
};

export type DownloadApiReq = {
  file_id: string;
};

export type DownloadApiResp = {
  status: string;
  file_id: string;
  original_hash: string;
  restored_hash: string;
  equal: boolean;
  bytes: number;
  file_bytes_base64: string;
};

export type RepairApiReq = {
  file_id: string;
  replication_factor?: number;
};

export type RepairApiResp = {
  status: string;
  file_id: string;
  target_replication_factor: number;
  repaired_shards: number;
  new_replicas: number;
};

export type WorkerAgentConfig = {
  worker_id: string;
  profile: string;
  listen_multiaddr: string;
  advertise_multiaddr: string;
  satellite_url: string;
  device_name: string;
  owner_label: string;
  capacity_limit_bytes: number;
  enabled: boolean;
};

export type WorkerAgentSatelliteView = {
  worker_id: string;
  multiaddr: string;
  device_name: string;
  owner_label: string;
  enabled: boolean;
};

export type WorkerAgentStatusResp = {
  running: boolean;
  pid: number | null;
  started_at_ms: number | null;
  last_exit_code: number | null;
  last_error: string | null;
  config: WorkerAgentConfig;
  satellite: WorkerAgentSatelliteView | null;
  identity_match: boolean | null;
};

export type WorkerAgentStorageResp = {
  profile: string;
  used_bytes: number;
  hosted_shards: number;
};

export function satelliteBaseUrl(): string {
  return process.env.SATELLITE_URL?.trim() || "http://127.0.0.1:7070";
}

export function localAgentBaseUrl(): string {
  return process.env.LOCAL_AGENT_URL?.trim() || "http://127.0.0.1:7081";
}

export async function fetchJson<T>(url: string): Promise<T> {
  const res = await fetch(url, { cache: "no-store" });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`${res.status} ${res.statusText}${body ? ` - ${body}` : ""}`);
  }
  return (await res.json()) as T;
}

export async function postJson<TReq>(url: string, payload: TReq): Promise<void> {
  const res = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    cache: "no-store",
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`${res.status} ${res.statusText}${body ? ` - ${body}` : ""}`);
  }
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let idx = 0;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }
  return `${value.toFixed(idx === 0 ? 0 : 2)} ${units[idx]}`;
}
