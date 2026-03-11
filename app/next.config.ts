import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  allowedDevOrigins: ["10.64.23.40"],
  experimental: {
    // Temporary local-dev increase while upload still flows through Server Actions.
    // Long-term: move large file uploads to a dedicated Route Handler / API endpoint.
    serverActions: {
      bodySizeLimit: "50mb",
    },
    // Needed for multipart/form-data buffering path before Server Actions parsing.
    proxyClientMaxBodySize: "50mb",
  },
};

export default nextConfig;
