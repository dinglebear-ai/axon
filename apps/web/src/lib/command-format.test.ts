import { describe, expect, it } from 'vitest';
import { formatCommandResponse, isPreviewableRasterArtifact, panelArtifactUrl } from './command-format';
import type { ArtifactHandle } from './panel-types';

describe('artifact preview URLs', () => {
  it('uses the opaque panel artifact content route', () => {
    expect(panelArtifactUrl('art_screenshot_123')).toBe(
      '/api/panel/artifacts/art_screenshot_123/content'
    );
  });

  it('uses the panel artifact route for screenshot previews and keeps the artifact row', () => {
    const view = formatCommandResponse({
      command: 'screenshot https://example.com',
      action: { action: 'screenshot' },
      result: {
        artifact_id: 'art_screenshot_123',
        artifact_kind: 'screenshot',
        width: 1280,
        height: 720,
        captured_at: '2026-07-16T00:00:00Z',
        warnings: []
      }
    });

    expect(view.imageUrl).toBe('/api/panel/artifacts/art_screenshot_123/content');
    expect(view.imageArtifact?.artifact_id).toBe('art_screenshot_123');
    expect(view.artifacts?.[0]?.artifact_id).toBe('art_screenshot_123');
    expect(view.raw).toBeUndefined();
  });

  it('does not create an image for non-image artifacts', () => {
    const view = formatCommandResponse({
      command: 'crawl https://example.com',
      action: { action: 'crawl' },
      result: {
        predicted_artifact_handles: [
          {
            artifact_id: 'art_report_123',
            artifact_kind: 'report',
            bytes: 64
          }
        ]
      }
    });

    expect(view.imageUrl).toBeUndefined();
    expect(view.artifacts?.[0].artifact_id).toBe('art_report_123');
  });

  it('only previews raster image artifact kinds', () => {
    const cases: Array<[ArtifactHandle, boolean]> = [
      [{ artifact_kind: 'screenshot', artifact_id: 'art_screenshot_1' }, true],
      [{ artifact_kind: 'image/png', artifact_id: 'art_image_1' }, true],
      [{ artifact_kind: 'report', artifact_id: 'art_report_1' }, false],
      [{ artifact_kind: 'image/svg+xml', artifact_id: 'art_image_2' }, false],
      [{ artifact_kind: 'screenshot', artifact_id: 'art_screenshot_2', bytes: 9 * 1024 * 1024 }, false]
    ];

    for (const [artifact, expected] of cases) {
      expect(isPreviewableRasterArtifact(artifact)).toBe(expected);
    }
  });
});
