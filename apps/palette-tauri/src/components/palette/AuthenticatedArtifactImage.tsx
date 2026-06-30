import { useCallback, useEffect, useRef, useState } from "react";

import { loadArtifactObjectUrl } from "@/lib/artifactPreview";

/**
 * Render an artifact preview image fetched through the authenticated Tauri
 * bridge. The blob object URL is owned here and revoked exactly once — via a
 * shared `activeUrlRef` consulted by both the effect cleanup and the `<img>`
 * onError handler — so a decode failure or unmount never leaks it.
 */
export function AuthenticatedArtifactImage({
  relativePath,
  alt,
}: {
  relativePath: string;
  alt: string;
}) {
  const [objectUrl, setObjectUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Single source of truth for the live blob URL so the effect cleanup and the
  // <img> onError handler revoke it exactly once (no double-revoke).
  const activeUrlRef = useRef<string | null>(null);

  const revokeActiveUrl = useCallback(() => {
    if (activeUrlRef.current) {
      URL.revokeObjectURL(activeUrlRef.current);
      activeUrlRef.current = null;
    }
  }, []);

  useEffect(() => {
    let cancelled = false;

    setObjectUrl(null);
    setError(null);

    loadArtifactObjectUrl(relativePath)
      .then((url) => {
        if (cancelled) {
          URL.revokeObjectURL(url);
          return;
        }
        activeUrlRef.current = url;
        setObjectUrl(url);
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      });

    return () => {
      cancelled = true;
      revokeActiveUrl();
    };
  }, [relativePath, revokeActiveUrl]);

  if (error) {
    return (
      <section className="operation-section">
        <p className="operation-muted">Preview unavailable: {error}</p>
      </section>
    );
  }
  if (!objectUrl) return null;
  return (
    <section className="operation-section">
      <figure className="operation-screenshot-preview">
        <img
          src={objectUrl}
          alt={alt}
          onError={() => {
            // The img is being replaced by the error text, so revoke its blob now
            // instead of waiting for the next effect run / unmount.
            revokeActiveUrl();
            setObjectUrl(null);
            setError("image decode failed");
          }}
        />
      </figure>
    </section>
  );
}
