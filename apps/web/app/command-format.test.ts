import { describe, expect, it } from 'vitest';
import { formatCommandResponse, isPreviewableRasterArtifact, panelArtifactUrl } from './command-format';
import type { ArtifactHandle } from './panel-types';

describe('artifact preview URLs', () => {
  it('segment-encodes panel artifact paths', () => {
    expect(panelArtifactUrl('screenshots/foo #1.png')).toBe(
      '/api/panel/artifact/screenshots/foo%20%231%2Epng'
    );
    expect(panelArtifactUrl('markdown/a%2Fb.md')).toBe('/api/panel/artifact/markdown/a%252Fb%2Emd');
    expect(panelArtifactUrl('screenshots/../secret.png')).toBe(
      '/api/panel/artifact/screenshots/%2E%2E/secret%2Epng'
    );
  });

  it('uses the panel artifact route for screenshot previews and keeps the artifact row', () => {
    const view = formatCommandResponse({
      command: 'screenshot https://example.com',
      action: { action: 'screenshot' },
      result: {
        url: 'https://example.com',
        path: '/home/axon/.axon/output/screenshots/example.png',
        size_bytes: 1024,
        artifact_handle: {
          relative_path: 'screenshots/example.png',
          display_path: 'screenshots/example.png',
          kind: 'screenshot',
          bytes: 1024
        }
      }
    });

    expect(view.imageUrl).toBe('/api/panel/artifact/screenshots/example%2Epng');
    expect(view.imageArtifact?.relative_path).toBe('screenshots/example.png');
    expect(view.artifacts?.[0]?.relative_path).toBe('screenshots/example.png');
    expect(view.raw).toBeUndefined();
  });

  it('does not create an image for non-image artifacts', () => {
    const view = formatCommandResponse({
      command: 'crawl https://example.com',
      action: { action: 'crawl' },
      result: {
        predicted_artifact_handles: [
          {
            relative_path: 'markdown/example.md',
            display_path: 'markdown/example.md',
            kind: 'markdown',
            bytes: 64
          }
        ]
      }
    });

    expect(view.imageUrl).toBeUndefined();
    expect(view.artifacts?.[0].relative_path).toBe('markdown/example.md');
    expect(view.artifacts?.[0].display_path).toBe('markdown/example.md');
  });

  it('only previews raster image artifact kinds and extensions', () => {
    const cases: Array<[ArtifactHandle, boolean]> = [
      [{ kind: 'screenshot', relative_path: 'screenshots/a.png', display_path: 'a.png' }, true],
      [{ kind: 'image/png', relative_path: 'screenshots/a.bin', display_path: 'a.bin' }, true],
      [{ kind: 'file', relative_path: 'screenshots/a.webp', display_path: 'a.webp' }, true],
      [{ kind: 'image/svg+xml', relative_path: 'icons/a.svg', display_path: 'a.svg' }, false],
      [{ kind: 'image/html', relative_path: 'page.html', display_path: 'page.html' }, false],
      [{ kind: 'file', relative_path: 'icons/a.svg', display_path: 'a.svg' }, false],
      [{ kind: 'screenshot', relative_path: 'screenshots/huge.png', display_path: 'huge.png', bytes: 9 * 1024 * 1024 }, false]
    ];

    for (const [artifact, expected] of cases) {
      expect(isPreviewableRasterArtifact(artifact)).toBe(expected);
    }
  });
});
