/**
 * Shared Content Security Policy builder.
 *
 * Both next.config.ts (static headers added at the edge/CDN layer) and
 * proxy.ts (dynamic per-request headers added in the middleware) must emit
 * identical CSP strings. This single source of truth prevents the two
 * definitions from silently diverging.
 *
 * Usage:
 *   import { buildCspHeader } from '@/lib/server/csp'
 *   const cspValue = buildCspHeader({ isDev, backendUrl })
 */

export interface CspOptions {
  /** True when NODE_ENV !== 'production'. Adds unsafe-eval and localhost sources. */
  isDev: boolean
  /**
   * Optional backend URL (e.g. http://localhost:49000). When provided, its
   * HTTP origin and matching ws:// / wss:// scheme are added to connect-src.
   */
  backendUrl?: string
  /**
   * Optional additional WebSocket URL (e.g. NEXT_PUBLIC_AXON_WS_URL).
   * When provided, its origin is added to connect-src.
   */
  wsUrl?: string
}

function buildConnectSources(options: CspOptions): string[] {
  const sources = new Set<string>(["'self'"])

  const urls = [options.backendUrl, options.wsUrl].filter(
    (value): value is string => typeof value === 'string' && value.length > 0,
  )

  for (const raw of urls) {
    try {
      const parsed = new URL(raw)
      sources.add(parsed.origin)
      const wsScheme = parsed.protocol === 'https:' ? 'wss:' : 'ws:'
      sources.add(`${wsScheme}//${parsed.host}`)
    } catch {
      // Ignore malformed URLs.
    }
  }

  if (options.isDev) {
    sources.add('http://localhost:*')
    sources.add('http://127.0.0.1:*')
    sources.add('ws://localhost:*')
    sources.add('ws://127.0.0.1:*')
  }

  return Array.from(sources)
}

/**
 * Build a complete Content-Security-Policy header value string.
 *
 * The returned string is ready to set directly as the header value — no
 * further joining or wrapping is needed.
 */
export function buildCspHeader(options: CspOptions): string {
  const { isDev } = options
  return [
    "default-src 'self'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-ancestors 'none'",
    "object-src 'none'",
    // 'unsafe-inline' is required in all environments: Next.js App Router
    // generates inline hydration scripts that would be blocked without it.
    // 'unsafe-eval' is dev-only (HMR/Turbopack). A future improvement could
    // replace 'unsafe-inline' with per-request nonces, but that requires
    // middleware integration to inject the nonce into both the CSP header
    // and each inline <script> tag.
    `script-src 'self' 'unsafe-inline'${isDev ? " 'unsafe-eval'" : ''}`,
    "style-src 'self' 'unsafe-inline'",
    // https: covers self-hosted backend screenshots served over HTTPS;
    // data: and blob: cover inline images and local file previews.
    "img-src 'self' data: blob: https:",
    "font-src 'self' data:",
    `connect-src ${buildConnectSources(options).join(' ')}`,
  ].join('; ')
}
