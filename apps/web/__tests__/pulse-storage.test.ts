import { describe, expect, it } from 'vitest'
import { listPulseDocs, loadPulseDoc, savePulseDoc, updatePulseDoc } from '@/lib/pulse/storage'

describe('pulse storage', () => {
  it('saves a pulse document to cache', async () => {
    const saved = await savePulseDoc({ title: 'Storage Test', markdown: '# Hello' })
    expect(saved.filename.endsWith('.md')).toBe(true)
    expect(saved.path.includes('.cache/pulse')).toBe(true)
  })

  it('loads a saved pulse document', async () => {
    const saved = await savePulseDoc({ title: 'Load Test', markdown: 'Body', tags: ['x'] })
    const loaded = await loadPulseDoc(saved.filename)
    expect(loaded?.title).toBe('Load Test')
    expect(loaded?.markdown.trim()).toBe('Body')
    expect(loaded?.tags).toContain('x')
  })

  it('returns null when document does not exist', async () => {
    const loaded = await loadPulseDoc('does-not-exist.md')
    expect(loaded).toBeNull()
  })

  it('lists docs sorted by updatedAt', async () => {
    await savePulseDoc({ title: 'List Test A', markdown: 'A' })
    await savePulseDoc({ title: 'List Test B', markdown: 'B' })
    const docs = await listPulseDocs()
    expect(docs.length).toBeGreaterThan(1)
    for (let i = 1; i < docs.length; i += 1) {
      expect(docs[i - 1]!.updatedAt >= docs[i]!.updatedAt).toBe(true)
    }
  })

  it('preserves collections metadata', async () => {
    const saved = await savePulseDoc({
      title: 'Collections Test',
      markdown: 'Body',
      collections: ['pulse', 'cortex'],
    })
    const loaded = await loadPulseDoc(saved.filename)
    expect(loaded?.collections).toEqual(['pulse', 'cortex'])
  })
})

describe('updatePulseDoc', () => {
  it('updates content in-place and preserves createdAt', async () => {
    const created = await savePulseDoc({ title: 'Update Test', markdown: 'Original' })
    const updated = await updatePulseDoc(created.filename, {
      title: 'Update Test',
      markdown: 'Revised',
    })

    expect(updated.filename).toBe(created.filename)
    expect(updated.createdAt).toBe(created.createdAt)
    expect(updated.updatedAt >= created.updatedAt).toBe(true)

    const loaded = await loadPulseDoc(created.filename)
    expect(loaded?.markdown.trim()).toBe('Revised')
    expect(loaded?.createdAt).toBe(created.createdAt)
  })

  it('preserves tags and collections from the original file when not supplied', async () => {
    const created = await savePulseDoc({
      title: 'Meta Preserve',
      markdown: 'Body',
      tags: ['alpha'],
      collections: ['cortex'],
    })
    const updated = await updatePulseDoc(created.filename, {
      title: 'Meta Preserve',
      markdown: 'Updated body',
    })

    expect(updated.tags).toEqual(['alpha'])
    expect(updated.collections).toEqual(['cortex'])
  })

  it('hasClientMeta fast-path: skips file read when createdAt/tags/collections provided', async () => {
    const created = await savePulseDoc({
      title: 'Fast Path',
      markdown: 'v1',
      tags: ['t'],
      collections: ['c'],
    })
    // Supply all three fast-path fields; omit clientUpdatedAt to skip conflict check.
    const updated = await updatePulseDoc(created.filename, {
      title: 'Fast Path',
      markdown: 'v2',
      createdAt: created.createdAt,
      tags: created.tags,
      collections: created.collections,
    })

    expect(updated.createdAt).toBe(created.createdAt)
    expect(updated.tags).toEqual(created.tags)

    const loaded = await loadPulseDoc(created.filename)
    expect(loaded?.markdown.trim()).toBe('v2')
  })

  it('creates a new file (phantom-create) when the target file does not exist', async () => {
    // updatePulseDoc should not throw when the file is missing — it creates it.
    const result = await updatePulseDoc('ghost-file-99999999999.md', {
      title: 'Ghost',
      markdown: 'Created from nothing',
    })

    expect(result.filename).toBe('ghost-file-99999999999.md')
    const loaded = await loadPulseDoc('ghost-file-99999999999.md')
    expect(loaded?.markdown.trim()).toBe('Created from nothing')
  })

  it('round-trip: save → update × 2 → load reflects only last update', async () => {
    const v1 = await savePulseDoc({ title: 'Round Trip', markdown: 'v1' })
    await updatePulseDoc(v1.filename, { title: 'Round Trip', markdown: 'v2' })
    await updatePulseDoc(v1.filename, { title: 'Round Trip', markdown: 'v3' })

    const loaded = await loadPulseDoc(v1.filename)
    expect(loaded?.markdown.trim()).toBe('v3')
    expect(loaded?.createdAt).toBe(v1.createdAt)
  })
})
