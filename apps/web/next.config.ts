import type { NextConfig } from 'next'

import { buildCspHeader } from './lib/server/csp'

const axonBackendUrl =
  process.env.AXON_BACKEND_URL || `http://localhost:${process.env.NEXT_PUBLIC_AXON_PORT || '49000'}`
const isDev = process.env.NODE_ENV !== 'production'

const securityHeaders = [
  { key: 'X-Frame-Options', value: 'DENY' },
  { key: 'X-Content-Type-Options', value: 'nosniff' },
  { key: 'Referrer-Policy', value: 'strict-origin-when-cross-origin' },
  { key: 'Permissions-Policy', value: 'camera=(), microphone=(), geolocation=()' },
  {
    key: 'Content-Security-Policy',
    // CSP is built by the shared lib/server/csp.ts module. proxy.ts uses the
    // same builder so both layers always emit the same policy string.
    value: buildCspHeader({
      isDev,
      backendUrl: axonBackendUrl,
      wsUrl: process.env.NEXT_PUBLIC_AXON_WS_URL,
    }),
  },
  ...(isDev
    ? []
    : [{ key: 'Strict-Transport-Security', value: 'max-age=31536000; includeSubDomains' }]),
]

const nextConfig: NextConfig = {
  output: 'standalone',
  allowedDevOrigins: ['axon.tootie.tv', 'localhost', '127.0.0.1', '10.1.0.6'],
  transpilePackages: [
    '@platejs/ai',
    '@platejs/diff',
    '@platejs/find-replace',
    '@platejs/basic-nodes',
    '@platejs/code-block',
    '@platejs/link',
    '@platejs/list',
    '@platejs/markdown',
    '@platejs/media',
    '@platejs/table',
    'platejs',
  ],
  turbopack: {
    root: __dirname,
  },
  images: {
    // S-M4: Restrict /_next/image proxy to HTTPS-only self-hosted origins.
    //
    // The wildcard `hostname: '**'` makes /_next/image?url=<any-url> an open
    // SSRF proxy. We lock it to the self-hosted Axon backend only.
    //
    // Editor image nodes (image-node.tsx, media-image-node-static.tsx) may
    // embed arbitrary user-supplied image URLs. Those components use
    // `unoptimized` or pass through the raw src, so they do NOT hit this proxy.
    // If a new component is added that proxies an external image, add its
    // specific hostname here — do not re-open the wildcard.
    remotePatterns: [
      {
        // Self-hosted Axon backend (screenshots, output files served over HTTPS)
        protocol: 'https',
        hostname: process.env.AXON_BACKEND_HOSTNAME ?? 'localhost',
      },
    ],
  },
  async headers() {
    return [
      {
        source: '/:path*',
        headers: securityHeaders,
      },
      {
        source: '/sw.js',
        headers: [
          { key: 'Cache-Control', value: 'no-cache, no-store, must-revalidate' },
          { key: 'Service-Worker-Allowed', value: '/' },
        ],
      },
      {
        source: '/api/cortex/:path*',
        headers: [
          { key: 'Cache-Control', value: 'public, s-maxage=30, stale-while-revalidate=60' },
        ],
      },
    ]
  },
  async rewrites() {
    return [
      {
        source: '/ws',
        destination: `${axonBackendUrl}/ws`,
      },
      {
        source: '/ws/shell',
        destination: `http://${process.env.SHELL_SERVER_HOST ?? '127.0.0.1'}:${process.env.SHELL_SERVER_PORT ?? 49011}`,
      },
      {
        source: '/download/:path*',
        destination: `${axonBackendUrl}/download/:path*`,
      },
      {
        source: '/output/:path*',
        destination: `${axonBackendUrl}/output/:path*`,
      },
    ]
  },
}

export default nextConfig
