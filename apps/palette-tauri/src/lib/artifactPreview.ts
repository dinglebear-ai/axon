import { invoke } from "./invoke";

const DEFAULT_ARTIFACT_CONTENT_TYPE = "application/octet-stream";

type ArtifactHttpResult = {
  ok: boolean;
  status: number;
  contentType: string;
  message?: string;
  bodyBase64: string;
};

export async function loadArtifactObjectUrl(artifactId: string): Promise<string> {
  try {
    const result = await invoke<ArtifactHttpResult>("axon_artifact_request", {
      artifactId,
    });
    if (!result.ok) {
      const detail = result.message?.trim();
      throw new Error(`artifact fetch failed with ${result.status}${detail ? `: ${detail}` : ""}`);
    }
    const blob = new Blob([decodeBase64(result.bodyBase64)], {
      type: result.contentType || DEFAULT_ARTIFACT_CONTENT_TYPE,
    });
    return URL.createObjectURL(blob);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`artifact preview load failed: ${message}`);
  }
}

function decodeBase64(value: string): ArrayBuffer {
  const decoded = atob(value);
  const bytes = new Uint8Array(decoded.length);
  for (let index = 0; index < decoded.length; index += 1) {
    bytes[index] = decoded.charCodeAt(index);
  }
  return bytes.buffer;
}
