import type { NextConfig } from "next";

// Where the Rust API runs, seen from the web server (not the browser).
const API_PROXY_URL = process.env.API_PROXY_URL ?? "http://127.0.0.1:8080";

const nextConfig: NextConfig = {
  /* config options here */
  cacheComponents: true,
  partialPrefetching: true,
  // The browser only talks to this web app; /api/... is passed on to the API.
  // So the app works from your phone over Tailscale, and the API never has
  // to listen on anything but 127.0.0.1.
  async rewrites() {
    return [{ source: "/api/:path*", destination: `${API_PROXY_URL}/api/:path*` }];
  },
  // Don't gzip: it would hold back the Ask page's streamed answers.
  compress: false,
  experimental: {
    // The assistant's first answer can take a minute while the model loads.
    proxyTimeout: 600_000,
  },
  turbopack: {
    rules: {
      "*.css": {
        loaders: ["@tailwindcss/turbopack"],
        as: "*.css",
      },
    },
  },
};

export default nextConfig;
