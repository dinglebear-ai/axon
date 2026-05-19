# Pulse Mode Phase 1 — Foundation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ship `Pulse` as a first-class workspace mode — omnibox-selectable, two-pane layout (PlateJS WYSIWYG editor + chat sidebar), real LLM copilot completions from day one, RAG-backed conversational editing, filesystem persistence with automatic Qdrant indexing.

**Architecture:** Pulse is a web-only mode (not a CLI command). When selected, the omnibox routes prompts to Next.js API routes instead of the WS executor. The editor pane hosts a full PlateJS instance with copilot ghost text powered by the real LLM endpoint (no mocks, no faker, ever). The chat pane sends prompts to `/api/pulse/chat`, which runs RAG retrieval against Qdrant collections, calls an LLM, and returns assistant text + citations + structured document operations. Saves persist to local filesystem and auto-embed into the `pulse` Qdrant collection.

**Tech Stack:** Next.js 16, React 19, Plate.js 52.x, @platejs/ai, @platejs/markdown, Tailwind v4, TypeScript 5.9, Zod 4

---

## Task 1: Copilot API route + remove faker — LLM completions from the start

The copilot route is foundational infrastructure. Everything else builds on a working LLM endpoint. No stubs, no mocks, no faker. This ships first.

**Files:**
- Create: `apps/web/lib/pulse/copilot-validation.ts`
- Create: `apps/web/app/api/ai/copilot/route.ts`
- Modify: `apps/web/components/editor/plugins/copilot-kit.tsx`
- Modify: `apps/web/package.json` (remove @faker-js/faker)

**Step 1: Write the failing test**

Create `apps/web/__tests__/api-copilot.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import { validateCopilotRequest } from '@/lib/pulse/copilot-validation'

describe('copilot request validation', () => {
  it('rejects empty prompt', () => {
    expect(validateCopilotRequest({ prompt: '' }).valid).toBe(false)
  })

  it('accepts valid prompt', () => {
    expect(validateCopilotRequest({ prompt: 'Continue: The quick brown' }).valid).toBe(true)
  })

  it('accepts prompt with optional system message', () => {
    const result = validateCopilotRequest({
      prompt: 'Continue this text',
      system: 'You are a writing assistant.',
    })
    expect(result.valid).toBe(true)
  })
})
```

**Step 2: Run the test to verify it fails**

```bash
cd apps/web && pnpm vitest run __tests__/api-copilot.test.ts
```

Expected: FAIL — module not found.

**Step 3: Create the validation helper**

Create `apps/web/lib/pulse/copilot-validation.ts`:

```typescript
import { z } from 'zod'

export const CopilotRequestSchema = z.object({
  prompt: z.string().min(1),
  system: z.string().optional(),
  model: z.string().optional(),
})

export function validateCopilotRequest(body: unknown) {
  const result = CopilotRequestSchema.safeParse(body)
  return { valid: result.success, error: result.error?.message }
}
```

**Step 4: Create the copilot API route**

Create `apps/web/app/api/ai/copilot/route.ts`:

```typescript
import { NextResponse } from 'next/server'
import { CopilotRequestSchema } from '@/lib/pulse/copilot-validation'

export async function POST(request: Request) {
  const baseUrl = process.env.OPENAI_BASE_URL
  const apiKey = process.env.OPENAI_API_KEY
  const model = process.env.OPENAI_MODEL ?? 'gpt-4o-mini'

  if (!baseUrl || !apiKey) {
    return NextResponse.json(
      { error: 'OPENAI_BASE_URL and OPENAI_API_KEY must be set' },
      { status: 503 },
    )
  }

  const body = await request.json()
  const parsed = CopilotRequestSchema.safeParse(body)

  if (!parsed.success) {
    return NextResponse.json({ error: parsed.error.message }, { status: 400 })
  }

  const { prompt, system } = parsed.data

  const response = await fetch(`${baseUrl}/chat/completions`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${apiKey}`,
    },
    body: JSON.stringify({
      model: parsed.data.model ?? model,
      messages: [
        ...(system ? [{ role: 'system' as const, content: system }] : []),
        { role: 'user' as const, content: prompt },
      ],
      max_tokens: 200,
      temperature: 0.7,
    }),
  })

  if (!response.ok) {
    const errText = await response.text()
    return NextResponse.json(
      { error: `LLM API error: ${response.status} ${errText}` },
      { status: 502 },
    )
  }

  const data = await response.json()
  const completion = data.choices?.[0]?.message?.content ?? ''

  return NextResponse.json({ completion })
}
```

**Step 5: Remove faker from copilot-kit.tsx — replace with real error handling**

In `apps/web/components/editor/plugins/copilot-kit.tsx`:

1. Delete `import { faker } from '@faker-js/faker'`
2. Replace the `onError` handler body:

```typescript
onError: (error) => {
  console.error('[Copilot] API error:', error)
},
```

The full CopilotKit array stays the same — it already points to `/api/ai/copilot` which now exists.

**Step 6: Remove the faker dependency entirely**

```bash
cd apps/web && pnpm remove @faker-js/faker
```

**Step 7: Verify no stray faker imports remain**

```bash
grep -r "faker" apps/web/components/ apps/web/lib/ apps/web/app/ --include="*.ts" --include="*.tsx"
```

Expected: no results.

**Step 8: Run the test to verify it passes**

```bash
cd apps/web && pnpm vitest run __tests__/api-copilot.test.ts
```

Expected: PASS.

**Step 9: Verify build**

```bash
cd apps/web && pnpm build
```

**Step 10: Commit**

```bash
git add apps/web/app/api/ai/copilot/route.ts \
  apps/web/lib/pulse/copilot-validation.ts \
  apps/web/components/editor/plugins/copilot-kit.tsx \
  apps/web/package.json apps/web/pnpm-lock.yaml \
  apps/web/__tests__/api-copilot.test.ts
git commit -m "feat(web): copilot API route with real LLM, remove faker entirely"
```

---

## Task 2: Install Zod + add `workspace` category and `pulse` mode to registry

**Files:**
- Modify: `apps/web/package.json` (add zod if not present)
- Modify: `apps/web/lib/ws-protocol.ts`

**Step 1: Install Zod if missing**

```bash
cd apps/web && grep '"zod"' package.json || pnpm add zod
```

**Step 2: Write the failing test**

Create `apps/web/__tests__/ws-protocol.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import {
  MODE_CATEGORY_LABELS,
  MODE_CATEGORY_ORDER,
  MODES,
  NO_INPUT_MODES,
  isWorkspaceMode,
} from '@/lib/ws-protocol'

describe('ws-protocol mode registry', () => {
  it('includes workspace in ModeCategory', () => {
    expect(MODE_CATEGORY_ORDER).toContain('workspace')
  })

  it('has a label for workspace category', () => {
    expect(MODE_CATEGORY_LABELS.workspace).toBe('Workspace')
  })

  it('places workspace after service in category order', () => {
    const idx = MODE_CATEGORY_ORDER.indexOf('workspace')
    const serviceIdx = MODE_CATEGORY_ORDER.indexOf('service')
    expect(idx).toBeGreaterThan(serviceIdx)
  })

  it('includes pulse mode in MODES', () => {
    const pulse = MODES.find((m) => m.id === 'pulse')
    expect(pulse).toBeDefined()
    expect(pulse!.category).toBe('workspace')
    expect(pulse!.label).toBe('Pulse')
  })

  it('pulse is NOT in NO_INPUT_MODES', () => {
    expect(NO_INPUT_MODES.has('pulse')).toBe(false)
  })

  it('pulse is a workspace mode', () => {
    expect(isWorkspaceMode('pulse')).toBe(true)
  })

  it('scrape is NOT a workspace mode', () => {
    expect(isWorkspaceMode('scrape')).toBe(false)
  })
})
```

**Step 3: Run the test to verify it fails**

```bash
cd apps/web && pnpm vitest run __tests__/ws-protocol.test.ts
```

Expected: FAIL — `workspace` not in category order, `pulse` not in MODES, `isWorkspaceMode` doesn't exist.

**Step 4: Implement all mode registry changes**

In `apps/web/lib/ws-protocol.ts`:

1. Update `ModeCategory` type:
```typescript
export type ModeCategory = 'content' | 'rag' | 'ingest' | 'ops' | 'service' | 'workspace'
```

2. Add `pulse` mode entry at the end of `MODES` (after `debug`):
```typescript
  // --- workspace ---
  {
    id: 'pulse',
    label: 'Pulse',
    category: 'workspace',
    icon: 'M13 10V3L4 14h7v7l9-11h-7z',
  },
```

3. Add `workspace` to `MODE_CATEGORY_LABELS`:
```typescript
export const MODE_CATEGORY_LABELS: Record<ModeCategory, string> = {
  content: 'Content',
  rag: 'RAG',
  ingest: 'Ingest',
  ops: 'Ops',
  service: 'Service',
  workspace: 'Workspace',
}
```

4. Add `workspace` to `MODE_CATEGORY_ORDER`:
```typescript
export const MODE_CATEGORY_ORDER: readonly ModeCategory[] = [
  'content',
  'rag',
  'ingest',
  'ops',
  'service',
  'workspace',
]
```

5. Add `isWorkspaceMode` helper:
```typescript
/** Modes in the workspace category bypass the WS executor entirely. */
export function isWorkspaceMode(id: string): boolean {
  const mode = MODES.find((m) => m.id === id)
  return mode?.category === 'workspace'
}
```

**Step 5: Run the test to verify it passes**

```bash
cd apps/web && pnpm vitest run __tests__/ws-protocol.test.ts
```

Expected: PASS — all 7 assertions green.

**Step 6: Verify build**

```bash
cd apps/web && pnpm build
```

**Step 7: Commit**

```bash
git add apps/web/lib/ws-protocol.ts apps/web/__tests__/ws-protocol.test.ts \
  apps/web/package.json apps/web/pnpm-lock.yaml
git commit -m "feat(web): workspace category + pulse mode + isWorkspaceMode helper"
```

---

## Task 3: Add `pulse` command spec + route omnibox away from WS executor

**Files:**
- Modify: `apps/web/lib/axon-command-map.ts`
- Modify: `apps/web/components/omnibox.tsx`
- Modify: `apps/web/hooks/use-ws-messages.ts`

**Step 1: Write the failing test**

Create `apps/web/__tests__/command-map.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import { getCommandSpec } from '@/lib/axon-command-map'

describe('axon-command-map: pulse', () => {
  it('has a command spec for pulse', () => {
    const spec = getCommandSpec('pulse')
    expect(spec).toBeDefined()
  })

  it('pulse spec has correct properties', () => {
    const spec = getCommandSpec('pulse')!
    expect(spec.category).toBe('workspace')
    expect(spec.input).toBe('text')
    expect(spec.asyncByDefault).toBe(false)
    expect(spec.supportsJobs).toBe(false)
    expect(spec.renderIntent).toBe('workspace')
  })
})
```

**Step 2: Run the test to verify it fails**

```bash
cd apps/web && pnpm vitest run __tests__/command-map.test.ts
```

Expected: FAIL — `getCommandSpec('pulse')` returns undefined.

**Step 3: Add the command spec**

In `apps/web/lib/axon-command-map.ts`:

1. Add `'workspace'` to `AxonCommandCategory`:
```typescript
export type AxonCommandCategory = 'content' | 'rag' | 'ingest' | 'ops' | 'service' | 'workspace'
```

2. Add `'workspace'` to `AxonRenderIntent`:
```typescript
export type AxonRenderIntent =
  | 'markdown-document'
  | 'manifest-browser'
  | 'table'
  | 'cards'
  | 'report'
  | 'job-lifecycle'
  | 'status-summary'
  | 'raw-fallback'
  | 'workspace'
```

3. Add pulse spec to `AXON_COMMAND_SPECS`:
```typescript
  // --- workspace ---
  {
    id: 'pulse',
    category: 'workspace',
    input: 'text',
    asyncByDefault: false,
    supportsJobs: false,
    commandOptions: [],
    renderIntent: 'workspace',
  },
```

**Step 4: Add workspace state to use-ws-messages.ts**

In `apps/web/hooks/use-ws-messages.ts`:

Add to `WsMessagesContextValue`:
```typescript
workspaceMode: string | null
workspacePrompt: string | null
activateWorkspace: (mode: string) => void
submitWorkspacePrompt: (prompt: string) => void
deactivateWorkspace: () => void
```

Add state + callbacks in `useWsMessagesProvider()`:
```typescript
const [workspaceMode, setWorkspaceMode] = useState<string | null>(null)
const [workspacePrompt, setWorkspacePrompt] = useState<string | null>(null)

const activateWorkspace = useCallback((mode: string) => {
  setWorkspaceMode(mode)
  setWorkspacePrompt(null)
}, [])

const submitWorkspacePrompt = useCallback((prompt: string) => {
  setWorkspacePrompt(prompt)
}, [])

const deactivateWorkspace = useCallback(() => {
  setWorkspaceMode(null)
  setWorkspacePrompt(null)
}, [])
```

Return all five from the hook.

**Step 5: Route omnibox for workspace modes**

In `apps/web/components/omnibox.tsx`:

1. Import `isWorkspaceMode`:
```typescript
import { isWorkspaceMode } from '@/lib/ws-protocol'
```

2. Destructure workspace actions from context at component top level:
```typescript
const { startExecution, activateWorkspace, submitWorkspacePrompt } = useWsMessages()
```

3. In `executeCommand`, add workspace branch at the top (before existing logic):
```typescript
if (isWorkspaceMode(execMode)) {
  activateWorkspace(execMode)
  if (execInput.trim()) submitWorkspacePrompt(execInput.trim())
  return
}
```

4. In `selectMode`, handle workspace modes:
```typescript
if (isWorkspaceMode(id)) {
  activateWorkspace(id)
} else if (NO_INPUT_MODES.has(id)) {
  setTimeout(() => executeCommand(id, ''), 0)
} else {
  inputRef.current?.focus()
}
```

**Step 6: Run test to verify it passes**

```bash
cd apps/web && pnpm vitest run __tests__/command-map.test.ts
```

Expected: PASS.

**Step 7: Verify build**

```bash
cd apps/web && pnpm build
```

**Step 8: Commit**

```bash
git add apps/web/lib/axon-command-map.ts apps/web/hooks/use-ws-messages.ts \
  apps/web/components/omnibox.tsx apps/web/__tests__/command-map.test.ts
git commit -m "feat(web): pulse command spec + omnibox workspace routing"
```

---

## Task 4: Pulse type schemas — doc ops, chat request/response, permissions

**Files:**
- Create: `apps/web/lib/pulse/types.ts`

**Step 1: Write the failing test**

Create `apps/web/__tests__/pulse-types.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import {
  DocOperationSchema,
  PulseChatRequestSchema,
  PulsePermissionLevel,
  type DocOperation,
} from '@/lib/pulse/types'

describe('pulse types', () => {
  it('validates a replace_document op', () => {
    const op: DocOperation = { type: 'replace_document', markdown: '# Hello' }
    expect(DocOperationSchema.parse(op)).toEqual(op)
  })

  it('validates an append_markdown op', () => {
    const op: DocOperation = { type: 'append_markdown', markdown: 'Some text' }
    expect(DocOperationSchema.parse(op)).toEqual(op)
  })

  it('validates an insert_section op', () => {
    const op: DocOperation = {
      type: 'insert_section',
      heading: 'New Section',
      markdown: 'Content here',
      position: 'bottom',
    }
    expect(DocOperationSchema.parse(op)).toEqual(op)
  })

  it('rejects unknown op types', () => {
    expect(() =>
      DocOperationSchema.parse({ type: 'delete_everything', markdown: '' })
    ).toThrow()
  })

  it('validates a chat request', () => {
    const req = {
      prompt: 'Add a summary section',
      documentMarkdown: '# Doc\n\nContent here',
      selectedCollections: ['pulse', 'cortex'],
    }
    expect(PulseChatRequestSchema.parse(req)).toBeTruthy()
  })

  it('rejects empty prompt', () => {
    expect(() => PulseChatRequestSchema.parse({ prompt: '' })).toThrow()
  })

  it('rejects prompt over 8000 chars', () => {
    expect(() => PulseChatRequestSchema.parse({ prompt: 'X'.repeat(8001) })).toThrow()
  })

  it('permission levels are correct', () => {
    expect(PulsePermissionLevel.options).toEqual(['plan', 'training-wheels', 'full-access'])
  })
})
```

**Step 2: Run the test to verify it fails**

```bash
cd apps/web && pnpm vitest run __tests__/pulse-types.test.ts
```

Expected: FAIL — module not found.

**Step 3: Implement the types**

Create `apps/web/lib/pulse/types.ts`:

```typescript
import { z } from 'zod'

// --- Document Operations ---

const ReplaceDocumentSchema = z.object({
  type: z.literal('replace_document'),
  markdown: z.string().min(1),
})

const AppendMarkdownSchema = z.object({
  type: z.literal('append_markdown'),
  markdown: z.string().min(1),
})

const InsertSectionSchema = z.object({
  type: z.literal('insert_section'),
  heading: z.string().min(1),
  markdown: z.string(),
  position: z.enum(['top', 'bottom']),
})

export const DocOperationSchema = z.discriminatedUnion('type', [
  ReplaceDocumentSchema,
  AppendMarkdownSchema,
  InsertSectionSchema,
])

export type DocOperation = z.infer<typeof DocOperationSchema>

// --- Permission Levels ---

export const PulsePermissionLevel = z.enum(['plan', 'training-wheels', 'full-access'])
export type PulsePermissionLevel = z.infer<typeof PulsePermissionLevel>

// --- Chat Request / Response ---

export const PulseChatRequestSchema = z.object({
  prompt: z.string().min(1).max(8000),
  documentMarkdown: z.string().max(100_000).default(''),
  selectedCollections: z.array(z.string()).default(['pulse']),
  conversationHistory: z
    .array(
      z.object({
        role: z.enum(['user', 'assistant']),
        content: z.string(),
      }),
    )
    .default([]),
  permissionLevel: PulsePermissionLevel.default('training-wheels'),
})

export type PulseChatRequest = z.infer<typeof PulseChatRequestSchema>

export interface PulseCitation {
  url: string
  title: string
  snippet: string
  collection: string
  score: number
}

export interface PulseChatResponse {
  text: string
  citations: PulseCitation[]
  operations: DocOperation[]
}

// --- Document Model ---

export interface PulseDocument {
  id: string
  title: string
  markdown: string
  createdAt: string
  updatedAt: string
  selectedCollections: string[]
  tags: string[]
}
```

**Step 4: Run test to verify it passes**

```bash
cd apps/web && pnpm vitest run __tests__/pulse-types.test.ts
```

Expected: PASS.

**Step 5: Verify build**

```bash
cd apps/web && pnpm build
```

**Step 6: Commit**

```bash
git add apps/web/lib/pulse/types.ts apps/web/__tests__/pulse-types.test.ts
git commit -m "feat(web): pulse type schemas — doc ops, chat request/response, permissions"
```

---

## Task 5: Doc ops validator + permissions model

**Files:**
- Create: `apps/web/lib/pulse/doc-ops.ts`
- Create: `apps/web/lib/pulse/permissions.ts`

**Step 1: Write the failing tests**

Create `apps/web/__tests__/pulse-doc-ops.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import type { DocOperation } from '@/lib/pulse/types'
import { isHighRiskOperationSet, validateDocOperations } from '@/lib/pulse/doc-ops'

describe('doc-ops validator', () => {
  it('accepts a single small append', () => {
    const ops: DocOperation[] = [{ type: 'append_markdown', markdown: 'Short text.' }]
    const result = validateDocOperations(ops, '# Existing doc content here')
    expect(result.valid).toBe(true)
    expect(result.reasons).toHaveLength(0)
  })

  it('flags replace_document changing >40% of chars', () => {
    const original = 'A'.repeat(1000)
    const ops: DocOperation[] = [
      { type: 'replace_document', markdown: 'B'.repeat(1000) },
    ]
    expect(isHighRiskOperationSet(ops, original)).toBe(true)
  })

  it('flags single insert >1200 chars', () => {
    const ops: DocOperation[] = [
      { type: 'append_markdown', markdown: 'X'.repeat(1201) },
    ]
    expect(isHighRiskOperationSet(ops, '')).toBe(true)
  })

  it('flags >3 operations in one set', () => {
    const ops: DocOperation[] = [
      { type: 'append_markdown', markdown: 'a' },
      { type: 'append_markdown', markdown: 'b' },
      { type: 'append_markdown', markdown: 'c' },
      { type: 'append_markdown', markdown: 'd' },
    ]
    expect(isHighRiskOperationSet(ops, '')).toBe(true)
  })

  it('flags ops that remove a heading', () => {
    const original = '# Title\n\nContent\n\n## Section\n\nMore'
    const ops: DocOperation[] = [
      { type: 'replace_document', markdown: '# Title\n\nContent\n\nMore' },
    ]
    const result = validateDocOperations(ops, original)
    expect(result.reasons).toContain('removes_heading')
  })

  it('small safe ops are not high risk', () => {
    const ops: DocOperation[] = [
      { type: 'append_markdown', markdown: 'Just a short note.' },
    ]
    expect(isHighRiskOperationSet(ops, '# Existing doc')).toBe(false)
  })
})
```

Create `apps/web/__tests__/pulse-permissions.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import type { DocOperation } from '@/lib/pulse/types'
import { checkPermission } from '@/lib/pulse/permissions'

describe('pulse permissions', () => {
  it('plan mode: allows ops on current document', () => {
    const ops: DocOperation[] = [{ type: 'append_markdown', markdown: 'note' }]
    const result = checkPermission('plan', ops, { isCurrentDoc: true })
    expect(result.allowed).toBe(true)
  })

  it('plan mode: blocks ops on other documents', () => {
    const ops: DocOperation[] = [{ type: 'append_markdown', markdown: 'note' }]
    const result = checkPermission('plan', ops, { isCurrentDoc: false })
    expect(result.allowed).toBe(false)
  })

  it('training-wheels mode: requires confirmation for high-risk ops', () => {
    const ops: DocOperation[] = [{ type: 'replace_document', markdown: 'X'.repeat(2000) }]
    const result = checkPermission('training-wheels', ops, {
      isCurrentDoc: true,
      currentDocMarkdown: 'A'.repeat(100),
    })
    expect(result.allowed).toBe(true)
    expect(result.requiresConfirmation).toBe(true)
  })

  it('full-access mode: allows everything without confirmation', () => {
    const ops: DocOperation[] = [{ type: 'replace_document', markdown: 'anything' }]
    const result = checkPermission('full-access', ops, { isCurrentDoc: false })
    expect(result.allowed).toBe(true)
    expect(result.requiresConfirmation).toBe(false)
  })
})
```

**Step 2: Run tests to verify they fail**

```bash
cd apps/web && pnpm vitest run __tests__/pulse-doc-ops.test.ts __tests__/pulse-permissions.test.ts
```

Expected: FAIL — modules not found.

**Step 3: Implement doc-ops.ts**

Create `apps/web/lib/pulse/doc-ops.ts`:

```typescript
import type { DocOperation } from './types'

const REPLACE_CHAR_THRESHOLD = 0.4
const MAX_INSERT_CHARS = 1200
const MAX_OPS_PER_RESPONSE = 3

export interface ValidationResult {
  valid: boolean
  reasons: string[]
}

function extractHeadings(markdown: string): string[] {
  return markdown
    .split('\n')
    .filter((line) => /^#{1,6}\s/.test(line))
    .map((line) => line.trim())
}

function detectHeadingRemoval(original: string, replacement: string): boolean {
  const originalHeadings = extractHeadings(original)
  const newHeadings = new Set(extractHeadings(replacement))
  return originalHeadings.some((h) => !newHeadings.has(h))
}

export function validateDocOperations(
  ops: DocOperation[],
  currentDocMarkdown: string,
): ValidationResult {
  const reasons: string[] = []

  if (ops.length > MAX_OPS_PER_RESPONSE) {
    reasons.push('too_many_ops')
  }

  for (const op of ops) {
    if (op.type === 'append_markdown' && op.markdown.length > MAX_INSERT_CHARS) {
      reasons.push('large_insert')
    }
    if (op.type === 'insert_section' && op.markdown.length > MAX_INSERT_CHARS) {
      reasons.push('large_insert')
    }
    if (op.type === 'replace_document' && currentDocMarkdown.length > 0) {
      const charDiff = Math.abs(op.markdown.length - currentDocMarkdown.length)
      const changeRatio = charDiff / currentDocMarkdown.length
      if (changeRatio > REPLACE_CHAR_THRESHOLD) {
        reasons.push('large_replace')
      }
      if (detectHeadingRemoval(currentDocMarkdown, op.markdown)) {
        reasons.push('removes_heading')
      }
    }
  }

  return { valid: reasons.length === 0, reasons }
}

export function isHighRiskOperationSet(
  ops: DocOperation[],
  currentDocMarkdown: string,
): boolean {
  return !validateDocOperations(ops, currentDocMarkdown).valid
}
```

**Step 4: Implement permissions.ts**

Create `apps/web/lib/pulse/permissions.ts`:

```typescript
import type { DocOperation, PulsePermissionLevel } from './types'
import { isHighRiskOperationSet } from './doc-ops'

export interface PermissionContext {
  isCurrentDoc: boolean
  currentDocMarkdown?: string
}

export interface PermissionResult {
  allowed: boolean
  requiresConfirmation: boolean
  reason?: string
}

export function checkPermission(
  level: PulsePermissionLevel,
  ops: DocOperation[],
  ctx: PermissionContext,
): PermissionResult {
  switch (level) {
    case 'plan':
      if (!ctx.isCurrentDoc) {
        return { allowed: false, requiresConfirmation: false, reason: 'plan_scope_violation' }
      }
      return { allowed: true, requiresConfirmation: false }

    case 'training-wheels': {
      const highRisk = isHighRiskOperationSet(ops, ctx.currentDocMarkdown ?? '')
      return { allowed: true, requiresConfirmation: highRisk }
    }

    case 'full-access':
      return { allowed: true, requiresConfirmation: false }
  }
}
```

**Step 5: Run tests to verify they pass**

```bash
cd apps/web && pnpm vitest run __tests__/pulse-doc-ops.test.ts __tests__/pulse-permissions.test.ts
```

Expected: PASS — all 10 assertions green.

**Step 6: Commit**

```bash
git add apps/web/lib/pulse/doc-ops.ts apps/web/lib/pulse/permissions.ts \
  apps/web/__tests__/pulse-doc-ops.test.ts apps/web/__tests__/pulse-permissions.test.ts
git commit -m "feat(web): doc ops validator + permissions model with guardrail thresholds"
```

---

## Task 6: RAG adapter — search collections via Qdrant + context window builder

**Files:**
- Create: `apps/web/lib/pulse/rag.ts`

**Step 1: Write the failing test**

Create `apps/web/__tests__/pulse-rag.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import { buildContextWindow } from '@/lib/pulse/rag'

describe('pulse RAG adapter', () => {
  it('buildContextWindow truncates to budget', () => {
    const chunks = [
      { text: 'A'.repeat(500), score: 0.9, url: 'https://a.com', collection: 'pulse' },
      { text: 'B'.repeat(500), score: 0.8, url: 'https://b.com', collection: 'cortex' },
      { text: 'C'.repeat(500), score: 0.7, url: 'https://c.com', collection: 'pulse' },
    ]
    const result = buildContextWindow(chunks, 1000)
    expect(result.text.length).toBeLessThanOrEqual(1200) // budget + formatting overhead
    expect(result.includedChunks).toBeLessThanOrEqual(3)
  })

  it('buildContextWindow returns empty for no chunks', () => {
    const result = buildContextWindow([], 1000)
    expect(result.text).toBe('')
    expect(result.includedChunks).toBe(0)
  })

  it('buildContextWindow preserves score order', () => {
    const chunks = [
      { text: 'Low', score: 0.3, url: 'https://low.com', collection: 'pulse' },
      { text: 'High', score: 0.95, url: 'https://high.com', collection: 'cortex' },
    ]
    const result = buildContextWindow(chunks, 10000)
    expect(result.text.indexOf('High')).toBeLessThan(result.text.indexOf('Low'))
  })
})
```

**Step 2: Run test to verify it fails**

```bash
cd apps/web && pnpm vitest run __tests__/pulse-rag.test.ts
```

**Step 3: Implement the RAG adapter**

Create `apps/web/lib/pulse/rag.ts`:

```typescript
export interface RagChunk {
  text: string
  score: number
  url: string
  collection: string
}

export interface ContextWindow {
  text: string
  includedChunks: number
  citations: Array<{ url: string; collection: string; score: number }>
}

export function buildContextWindow(chunks: RagChunk[], budget: number): ContextWindow {
  const sorted = [...chunks].sort((a, b) => b.score - a.score)

  let totalLen = 0
  const included: RagChunk[] = []

  for (const chunk of sorted) {
    if (totalLen + chunk.text.length > budget) {
      const remaining = budget - totalLen
      if (remaining > 100) {
        included.push({ ...chunk, text: chunk.text.slice(0, remaining) })
        totalLen += remaining
      }
      break
    }
    included.push(chunk)
    totalLen += chunk.text.length
  }

  const text = included.map((c) => `[Source: ${c.url}]\n${c.text}`).join('\n\n---\n\n')

  return {
    text,
    includedChunks: included.length,
    citations: included.map((c) => ({ url: c.url, collection: c.collection, score: c.score })),
  }
}

export async function searchCollections(
  query: string,
  collections: string[],
  limit: number = 10,
): Promise<RagChunk[]> {
  const qdrantUrl = process.env.QDRANT_URL
  const teiUrl = process.env.TEI_URL

  if (!qdrantUrl || !teiUrl) {
    console.error('[RAG] QDRANT_URL and TEI_URL must be set')
    return []
  }

  const embedResponse = await fetch(`${teiUrl}/embed`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ inputs: query }),
  })

  if (!embedResponse.ok) {
    console.error('[RAG] TEI embed failed:', embedResponse.status)
    return []
  }

  const embeddings: number[][] = await embedResponse.json()
  const vector = embeddings[0]
  if (!vector) return []

  const results = await Promise.all(
    collections.map(async (collection) => {
      const searchResponse = await fetch(`${qdrantUrl}/collections/${collection}/points/search`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ vector, limit, with_payload: true }),
      })

      if (!searchResponse.ok) {
        console.error(`[RAG] Qdrant search failed for ${collection}:`, searchResponse.status)
        return []
      }

      const data = await searchResponse.json()
      return (data.result ?? []).map((point: { score: number; payload?: Record<string, unknown> }) => ({
        text: String(point.payload?.text ?? point.payload?.content ?? ''),
        score: point.score,
        url: String(point.payload?.url ?? ''),
        collection,
      }))
    }),
  )

  return results.flat().sort((a, b) => b.score - a.score).slice(0, limit)
}
```

**Step 4: Run test to verify it passes**

```bash
cd apps/web && pnpm vitest run __tests__/pulse-rag.test.ts
```

Expected: PASS.

**Step 5: Commit**

```bash
git add apps/web/lib/pulse/rag.ts apps/web/__tests__/pulse-rag.test.ts
git commit -m "feat(web): RAG adapter — context window builder + Qdrant search"
```

---

## Task 7: Pulse chat API route + save/load routes + storage layer

**Files:**
- Create: `apps/web/lib/pulse/storage.ts`
- Create: `apps/web/app/api/pulse/chat/route.ts`
- Create: `apps/web/app/api/pulse/save/route.ts`
- Create: `apps/web/app/api/pulse/doc/route.ts`

**Step 1: Write the failing test for storage helpers**

Create `apps/web/__tests__/pulse-storage.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import { generateDocId, docFilename, parseDocFrontmatter } from '@/lib/pulse/storage'

describe('pulse storage helpers', () => {
  it('generates a doc ID as a timestamp-based string', () => {
    const id = generateDocId()
    expect(id).toMatch(/^\d{4}-\d{2}-\d{2}T/)
  })

  it('creates filename from title', () => {
    expect(docFilename('My First Plan')).toBe('my-first-plan.md')
  })

  it('handles special characters in title', () => {
    expect(docFilename('What is RAG? (Part 1)')).toBe('what-is-rag-part-1.md')
  })

  it('parses frontmatter from markdown', () => {
    const md = `---
title: Test Doc
tags: [research, planning]
collections: [pulse]
---

# Content here`
    const result = parseDocFrontmatter(md)
    expect(result.title).toBe('Test Doc')
    expect(result.tags).toEqual(['research', 'planning'])
    expect(result.body).toContain('# Content here')
  })

  it('handles markdown without frontmatter', () => {
    const md = '# Just a heading\n\nSome content'
    const result = parseDocFrontmatter(md)
    expect(result.title).toBe('')
    expect(result.body).toBe(md)
  })
})
```

**Step 2: Run test to verify it fails**

```bash
cd apps/web && pnpm vitest run __tests__/pulse-storage.test.ts
```

**Step 3: Implement storage.ts**

Create `apps/web/lib/pulse/storage.ts`:

```typescript
import { readdir, readFile, writeFile, mkdir } from 'node:fs/promises'
import { join } from 'node:path'

export const PULSE_DOCS_DIR = process.env.PULSE_DOCS_DIR ?? join(process.cwd(), '.cache', 'pulse')

export function generateDocId(): string {
  return new Date().toISOString()
}

export function docFilename(title: string): string {
  return (
    title
      .toLowerCase()
      .replace(/[^a-z0-9\s-]/g, '')
      .replace(/\s+/g, '-')
      .replace(/-+/g, '-')
      .replace(/^-|-$/g, '') + '.md'
  )
}

export function parseDocFrontmatter(markdown: string): {
  title: string
  tags: string[]
  collections: string[]
  body: string
} {
  const fmMatch = markdown.match(/^---\n([\s\S]*?)\n---\n?([\s\S]*)$/)
  if (!fmMatch) return { title: '', tags: [], collections: [], body: markdown }

  const frontmatter = fmMatch[1]
  const body = fmMatch[2].trim()

  const title = frontmatter.match(/^title:\s*(.+)$/m)?.[1]?.trim() ?? ''
  const tagsMatch = frontmatter.match(/^tags:\s*\[(.+)\]$/m)?.[1]
  const tags = tagsMatch ? tagsMatch.split(',').map((t) => t.trim()) : []
  const colsMatch = frontmatter.match(/^collections:\s*\[(.+)\]$/m)?.[1]
  const collections = colsMatch ? colsMatch.split(',').map((c) => c.trim()) : []

  return { title, tags, collections, body }
}

function buildFrontmatter(meta: {
  title: string
  tags: string[]
  collections: string[]
  updatedAt: string
}): string {
  return `---
title: ${meta.title}
tags: [${meta.tags.join(', ')}]
collections: [${meta.collections.join(', ')}]
updated_at: ${meta.updatedAt}
---`
}

export async function savePulseDoc(opts: {
  title: string
  markdown: string
  tags?: string[]
  collections?: string[]
}): Promise<{ path: string; filename: string }> {
  await mkdir(PULSE_DOCS_DIR, { recursive: true })

  const filename = docFilename(opts.title || 'untitled')
  const path = join(PULSE_DOCS_DIR, filename)
  const frontmatter = buildFrontmatter({
    title: opts.title,
    tags: opts.tags ?? [],
    collections: opts.collections ?? ['pulse'],
    updatedAt: new Date().toISOString(),
  })

  await writeFile(path, `${frontmatter}\n\n${opts.markdown}`, 'utf-8')
  return { path, filename }
}

export async function loadPulseDoc(filename: string) {
  try {
    const path = join(PULSE_DOCS_DIR, filename)
    const raw = await readFile(path, 'utf-8')
    const { title, tags, collections, body } = parseDocFrontmatter(raw)
    return { title, markdown: body, tags, collections }
  } catch {
    return null
  }
}

export async function listPulseDocs() {
  try {
    await mkdir(PULSE_DOCS_DIR, { recursive: true })
    const files = await readdir(PULSE_DOCS_DIR)
    return Promise.all(
      files.filter((f) => f.endsWith('.md')).map(async (filename) => {
        const raw = await readFile(join(PULSE_DOCS_DIR, filename), 'utf-8')
        const { title } = parseDocFrontmatter(raw)
        const updatedMatch = raw.match(/^updated_at:\s*(.+)$/m)
        return {
          filename,
          title: title || filename.replace('.md', ''),
          updatedAt: updatedMatch?.[1]?.trim() ?? '',
        }
      }),
    )
  } catch {
    return []
  }
}
```

**Step 4: Create the chat API route**

Create `apps/web/app/api/pulse/chat/route.ts`:

```typescript
import { NextResponse } from 'next/server'
import { DocOperationSchema, PulseChatRequestSchema } from '@/lib/pulse/types'
import type { PulseChatResponse } from '@/lib/pulse/types'
import { buildContextWindow, searchCollections } from '@/lib/pulse/rag'

const CONTEXT_BUDGET = 6000

export async function POST(request: Request) {
  const baseUrl = process.env.OPENAI_BASE_URL
  const apiKey = process.env.OPENAI_API_KEY
  const model = process.env.OPENAI_MODEL ?? 'gpt-4o-mini'

  if (!baseUrl || !apiKey) {
    return NextResponse.json(
      { error: 'OPENAI_BASE_URL and OPENAI_API_KEY must be set' },
      { status: 503 },
    )
  }

  const body = await request.json()
  const parsed = PulseChatRequestSchema.safeParse(body)

  if (!parsed.success) {
    return NextResponse.json({ error: parsed.error.message }, { status: 400 })
  }

  const { prompt, documentMarkdown, selectedCollections, conversationHistory, permissionLevel } =
    parsed.data

  const chunks = await searchCollections(prompt, selectedCollections, 10)
  const context = buildContextWindow(chunks, CONTEXT_BUDGET)

  const systemMessage = `You are Pulse, an AI writing assistant integrated into the Axon knowledge workspace.

You help users write, edit, and research documents. You have access to the user's knowledge base via RAG search results provided below.

Current document:
---
${documentMarkdown.slice(0, 4000)}
---

Retrieved context from knowledge base:
---
${context.text}
---

When the user asks you to modify the document, respond with:
1. Your conversational response explaining what you did
2. A JSON block with document operations (if applicable)

Document operation format (return as a JSON code block):
\`\`\`json
{"operations": [{"type": "append_markdown", "markdown": "..."}, ...]}
\`\`\`

Available operation types:
- replace_document: Replace entire document content
- append_markdown: Append text to end of document
- insert_section: Insert a new section with heading and content

Permission level: ${permissionLevel}
${permissionLevel === 'plan' ? 'You may ONLY modify the current document.' : ''}

Always cite sources when using retrieved context. Format: [Source Title](url)`

  const messages = [
    { role: 'system' as const, content: systemMessage },
    ...conversationHistory.map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content })),
    { role: 'user' as const, content: prompt },
  ]

  const llmResponse = await fetch(`${baseUrl}/chat/completions`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${apiKey}`,
    },
    body: JSON.stringify({ model, messages, max_tokens: 2000, temperature: 0.7 }),
  })

  if (!llmResponse.ok) {
    const errText = await llmResponse.text()
    return NextResponse.json({ error: `LLM API error: ${llmResponse.status} ${errText}` }, { status: 502 })
  }

  const llmData = await llmResponse.json()
  const assistantText = llmData.choices?.[0]?.message?.content ?? ''

  const operations = extractOperations(assistantText)
  const citations = context.citations.map((c) => ({
    url: c.url,
    title: c.url.split('/').pop() ?? c.url,
    snippet: '',
    collection: c.collection,
    score: c.score,
  }))

  const response: PulseChatResponse = { text: assistantText, citations, operations }
  return NextResponse.json(response)
}

function extractOperations(text: string): PulseChatResponse['operations'] {
  const jsonMatch = text.match(/```json\s*([\s\S]*?)```/)
  if (!jsonMatch) return []

  try {
    const parsed = JSON.parse(jsonMatch[1])
    const ops = Array.isArray(parsed.operations) ? parsed.operations : []
    return ops.filter((op: unknown) => DocOperationSchema.safeParse(op).success)
  } catch {
    return []
  }
}
```

**Step 5: Create save route**

Create `apps/web/app/api/pulse/save/route.ts`:

```typescript
import { NextResponse } from 'next/server'
import { z } from 'zod'
import { savePulseDoc } from '@/lib/pulse/storage'

const SaveRequestSchema = z.object({
  title: z.string().min(1).max(200),
  markdown: z.string().max(200_000),
  tags: z.array(z.string()).default([]),
  collections: z.array(z.string()).default(['pulse']),
  embed: z.boolean().default(true),
})

export async function POST(request: Request) {
  const body = await request.json()
  const parsed = SaveRequestSchema.safeParse(body)

  if (!parsed.success) {
    return NextResponse.json({ error: parsed.error.message }, { status: 400 })
  }

  const { title, markdown, tags, collections, embed } = parsed.data
  const { path, filename } = await savePulseDoc({ title, markdown, tags, collections })

  if (embed) {
    const teiUrl = process.env.TEI_URL
    const qdrantUrl = process.env.QDRANT_URL
    const collection = collections[0] ?? 'pulse'

    if (teiUrl && qdrantUrl) {
      try {
        const chunks = chunkText(markdown, 2000, 200)
        const embedResponse = await fetch(`${teiUrl}/embed`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ inputs: chunks }),
        })

        if (embedResponse.ok) {
          const vectors: number[][] = await embedResponse.json()
          const points = vectors.map((vector, i) => ({
            id: crypto.randomUUID(),
            vector,
            payload: {
              text: chunks[i],
              url: `pulse://${filename}`,
              title,
              doc_type: 'pulse_note',
              chunk_index: i,
            },
          }))

          await fetch(`${qdrantUrl}/collections/${collection}/points?wait=true`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ points }),
          })
        }
      } catch (err) {
        console.error('[Pulse] Embed failed (save succeeded):', err)
      }
    }
  }

  return NextResponse.json({ path, filename, saved: true })
}

function chunkText(text: string, size: number, overlap: number): string[] {
  const chunks: string[] = []
  let start = 0
  while (start < text.length) {
    chunks.push(text.slice(start, start + size))
    start += size - overlap
  }
  return chunks
}
```

**Step 6: Create doc load/list route**

Create `apps/web/app/api/pulse/doc/route.ts`:

```typescript
import { NextResponse } from 'next/server'
import { listPulseDocs, loadPulseDoc } from '@/lib/pulse/storage'

export async function GET(request: Request) {
  const url = new URL(request.url)
  const filename = url.searchParams.get('filename')

  if (filename) {
    const doc = await loadPulseDoc(filename)
    if (!doc) return NextResponse.json({ error: 'Not found' }, { status: 404 })
    return NextResponse.json(doc)
  }

  const docs = await listPulseDocs()
  return NextResponse.json({ docs })
}
```

**Step 7: Run storage tests**

```bash
cd apps/web && pnpm vitest run __tests__/pulse-storage.test.ts
```

Expected: PASS.

**Step 8: Verify build**

```bash
cd apps/web && pnpm build
```

**Step 9: Commit**

```bash
git add apps/web/lib/pulse/storage.ts apps/web/lib/pulse/rag.ts \
  apps/web/app/api/pulse/chat/route.ts apps/web/app/api/pulse/save/route.ts \
  apps/web/app/api/pulse/doc/route.ts apps/web/__tests__/pulse-storage.test.ts
git commit -m "feat(web): pulse chat + save + doc routes with RAG and Qdrant embed"
```

---

## Task 8: Pulse workspace shell — editor, chat, toolbar, confirmation modal

All UI components ship in one task. The editor uses CopilotKit (which now hits the real LLM from Task 1). The chat pane wires to `/api/pulse/chat`. Autosave wires to `/api/pulse/save`. Confirmation modal enforces guardrails.

**Files:**
- Create: `apps/web/components/pulse/pulse-workspace.tsx`
- Create: `apps/web/components/pulse/pulse-editor-pane.tsx`
- Create: `apps/web/components/pulse/pulse-chat-pane.tsx`
- Create: `apps/web/components/pulse/pulse-toolbar.tsx`
- Create: `apps/web/components/pulse/pulse-op-confirmation.tsx`

**Step 1: Create pulse-editor-pane.tsx**

```typescript
'use client'

import { usePlateEditor } from 'platejs/react'
import { Plate } from 'platejs/react'
import { Editor, EditorContainer } from '@/components/ui/editor'
import { CopilotKit } from '@/components/editor/plugins/copilot-kit'

interface PulseEditorPaneProps {
  markdown: string
  onMarkdownChange: (md: string) => void
}

export function PulseEditorPane({ markdown, onMarkdownChange }: PulseEditorPaneProps) {
  const editor = usePlateEditor({
    plugins: CopilotKit,
  })

  return (
    <Plate editor={editor}>
      <EditorContainer className="h-full">
        <Editor
          variant="fullWidth"
          placeholder="Start writing, or ask Pulse to help..."
          className="min-h-[50vh] p-4 text-sm"
        />
      </EditorContainer>
    </Plate>
  )
}
```

**Step 2: Create pulse-chat-pane.tsx**

```typescript
'use client'

import type { ChatMessage } from './pulse-workspace'

interface PulseChatPaneProps {
  messages: ChatMessage[]
  isLoading: boolean
}

export function PulseChatPane({ messages, isLoading }: PulseChatPaneProps) {
  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-[rgba(175,215,255,0.08)] px-4 py-2.5">
        <span className="text-[10px] font-bold uppercase tracking-[0.15em] text-[#5f87af]">
          Pulse Chat
        </span>
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {messages.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <p className="text-center text-xs text-[#5f87af]">
              Type a prompt in the omnibox to start.
              <br />
              Pulse will search your knowledge base and help edit this document.
            </p>
          </div>
        ) : (
          <div className="space-y-4">
            {messages.map((msg, i) => (
              <div
                key={`msg-${i}-${msg.role}`}
                className={`rounded-lg p-3 text-sm ${
                  msg.role === 'user'
                    ? 'ml-8 bg-[rgba(255,135,175,0.08)] text-[#e8f4f8]'
                    : 'mr-8 bg-[rgba(175,215,255,0.06)] text-[#c4daf0]'
                }`}
              >
                {msg.content}
              </div>
            ))}
          </div>
        )}
        {isLoading && (
          <div className="mt-3 flex items-center gap-2 text-xs text-[#5f87af]">
            <span className="inline-block size-1.5 animate-pulse rounded-full bg-[#ff87af]" />
            Thinking...
          </div>
        )}
      </div>
    </div>
  )
}
```

**Step 3: Create pulse-toolbar.tsx**

```typescript
'use client'

import type { PulsePermissionLevel } from '@/lib/pulse/types'

interface PulseToolbarProps {
  title: string
  onTitleChange: (title: string) => void
  permissionLevel: PulsePermissionLevel
  onPermissionChange: (level: PulsePermissionLevel) => void
  saveStatus: 'idle' | 'saving' | 'saved' | 'error'
}

const PERMISSION_OPTIONS: { value: PulsePermissionLevel; label: string }[] = [
  { value: 'plan', label: 'Plan' },
  { value: 'training-wheels', label: 'Training Wheels' },
  { value: 'full-access', label: 'Full Access' },
]

export function PulseToolbar({
  title,
  onTitleChange,
  permissionLevel,
  onPermissionChange,
  saveStatus,
}: PulseToolbarProps) {
  return (
    <div className="flex items-center justify-between rounded-lg border border-[rgba(175,215,255,0.08)] bg-[rgba(10,18,35,0.3)] px-3 py-1.5">
      <input
        value={title}
        onChange={(e) => onTitleChange(e.target.value)}
        className="bg-transparent text-sm font-medium text-[#e8f4f8] outline-none placeholder:text-[#475569]"
        placeholder="Document title..."
      />

      <div className="flex items-center gap-3">
        <span className="text-[10px] text-[#5f87af]">
          {saveStatus === 'saving' && 'Saving...'}
          {saveStatus === 'saved' && 'Saved'}
          {saveStatus === 'error' && 'Save failed'}
        </span>

        <div className="flex items-center gap-1">
          {PERMISSION_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() => onPermissionChange(opt.value)}
              className={`rounded-md px-2 py-1 text-[10px] font-semibold uppercase tracking-wider transition-colors ${
                permissionLevel === opt.value
                  ? 'bg-[rgba(255,135,175,0.15)] text-[#ff87af]'
                  : 'text-[#5f87af] hover:text-[#8787af]'
              }`}
              title={opt.label}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}
```

**Step 4: Create pulse-op-confirmation.tsx**

```typescript
'use client'

import type { DocOperation } from '@/lib/pulse/types'
import type { ValidationResult } from '@/lib/pulse/doc-ops'

interface PulseOpConfirmationProps {
  operations: DocOperation[]
  validation: ValidationResult
  onConfirm: () => void
  onReject: () => void
}

const REASON_LABELS: Record<string, string> = {
  too_many_ops: 'Multiple operations in one response',
  large_insert: 'Large text insertion (>1200 characters)',
  large_replace: 'Replaces more than 40% of the document',
  removes_heading: 'Removes one or more section headings',
}

export function PulseOpConfirmation({
  operations,
  validation,
  onConfirm,
  onReject,
}: PulseOpConfirmationProps) {
  return (
    <div className="rounded-lg border border-[rgba(255,135,175,0.3)] bg-[rgba(255,135,175,0.05)] p-4">
      <h4 className="mb-2 text-xs font-bold uppercase tracking-wider text-[#ff87af]">
        Confirm Document Changes
      </h4>
      <p className="mb-3 text-xs text-[#8787af]">
        The assistant wants to apply {operations.length} operation(s) that triggered safety checks:
      </p>
      <ul className="mb-3 space-y-1">
        {validation.reasons.map((reason) => (
          <li key={reason} className="text-xs text-[#c4daf0]">
            {REASON_LABELS[reason] ?? reason}
          </li>
        ))}
      </ul>
      <div className="flex gap-2">
        <button
          type="button"
          onClick={onConfirm}
          className="rounded-md bg-[rgba(255,135,175,0.2)] px-3 py-1.5 text-xs font-semibold text-[#ff87af] transition-colors hover:bg-[rgba(255,135,175,0.3)]"
        >
          Apply Changes
        </button>
        <button
          type="button"
          onClick={onReject}
          className="rounded-md bg-[rgba(175,215,255,0.1)] px-3 py-1.5 text-xs font-semibold text-[#8787af] transition-colors hover:text-[#afd7ff]"
        >
          Reject
        </button>
      </div>
    </div>
  )
}
```

**Step 5: Create pulse-workspace.tsx — the full orchestrator**

This is the main component. It wires everything together: editor, chat, autosave, prompt handling, doc ops, confirmation modal.

```typescript
'use client'

import { useEffect, useRef, useState } from 'react'
import { useWsMessages } from '@/hooks/use-ws-messages'
import { validateDocOperations } from '@/lib/pulse/doc-ops'
import type { ValidationResult } from '@/lib/pulse/doc-ops'
import { checkPermission } from '@/lib/pulse/permissions'
import type { DocOperation, PulseChatResponse, PulsePermissionLevel } from '@/lib/pulse/types'
import { PulseChatPane } from './pulse-chat-pane'
import { PulseEditorPane } from './pulse-editor-pane'
import { PulseOpConfirmation } from './pulse-op-confirmation'
import { PulseToolbar } from './pulse-toolbar'

export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
  citations?: PulseChatResponse['citations']
  operations?: PulseChatResponse['operations']
}

export function PulseWorkspace() {
  const { workspacePrompt } = useWsMessages()
  const [permissionLevel, setPermissionLevel] = useState<PulsePermissionLevel>('training-wheels')
  const [documentMarkdown, setDocumentMarkdown] = useState('')
  const [chatHistory, setChatHistory] = useState<ChatMessage[]>([])
  const [isChatLoading, setIsChatLoading] = useState(false)
  const [documentTitle, setDocumentTitle] = useState('Untitled')
  const [saveStatus, setSaveStatus] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle')
  const [pendingOps, setPendingOps] = useState<DocOperation[] | null>(null)
  const [pendingValidation, setPendingValidation] = useState<ValidationResult | null>(null)
  const autosaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const lastPromptRef = useRef<string | null>(null)

  // --- Prompt handling ---
  useEffect(() => {
    if (!workspacePrompt || workspacePrompt === lastPromptRef.current) return
    lastPromptRef.current = workspacePrompt

    const prompt = workspacePrompt
    setChatHistory((prev) => [...prev, { role: 'user', content: prompt }])
    setIsChatLoading(true)

    fetch('/api/pulse/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        prompt,
        documentMarkdown,
        selectedCollections: ['pulse', 'cortex'],
        conversationHistory: chatHistory.map((m) => ({ role: m.role, content: m.content })),
        permissionLevel,
      }),
    })
      .then((res) => res.json())
      .then((data: PulseChatResponse) => {
        setChatHistory((prev) => [
          ...prev,
          { role: 'assistant', content: data.text, citations: data.citations, operations: data.operations },
        ])

        if (data.operations.length > 0) {
          const perm = checkPermission(permissionLevel, data.operations, {
            isCurrentDoc: true,
            currentDocMarkdown: documentMarkdown,
          })

          if (perm.allowed && !perm.requiresConfirmation) {
            applyOperations(data.operations)
          } else if (perm.allowed && perm.requiresConfirmation) {
            const validation = validateDocOperations(data.operations, documentMarkdown)
            setPendingOps(data.operations)
            setPendingValidation(validation)
          }
        }
      })
      .catch((err) => {
        setChatHistory((prev) => [...prev, { role: 'assistant', content: `Error: ${err.message}` }])
      })
      .finally(() => setIsChatLoading(false))
  }, [workspacePrompt, documentMarkdown, chatHistory, permissionLevel])

  // --- Autosave ---
  useEffect(() => {
    if (!documentMarkdown || !documentTitle) return

    if (autosaveTimerRef.current) clearTimeout(autosaveTimerRef.current)
    autosaveTimerRef.current = setTimeout(() => {
      setSaveStatus('saving')
      fetch('/api/pulse/save', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ title: documentTitle, markdown: documentMarkdown, embed: true }),
      })
        .then((res) => {
          setSaveStatus(res.ok ? 'saved' : 'error')
          setTimeout(() => setSaveStatus('idle'), 2000)
        })
        .catch(() => setSaveStatus('error'))
    }, 1500)

    return () => {
      if (autosaveTimerRef.current) clearTimeout(autosaveTimerRef.current)
    }
  }, [documentMarkdown, documentTitle])

  function applyOperations(ops: DocOperation[]) {
    for (const op of ops) {
      switch (op.type) {
        case 'replace_document':
          setDocumentMarkdown(op.markdown)
          break
        case 'append_markdown':
          setDocumentMarkdown((prev) => `${prev}\n\n${op.markdown}`)
          break
        case 'insert_section':
          setDocumentMarkdown((prev) =>
            op.position === 'top'
              ? `## ${op.heading}\n\n${op.markdown}\n\n${prev}`
              : `${prev}\n\n## ${op.heading}\n\n${op.markdown}`,
          )
          break
      }
    }
  }

  return (
    <div className="mt-3 flex flex-col gap-2">
      <PulseToolbar
        title={documentTitle}
        onTitleChange={setDocumentTitle}
        permissionLevel={permissionLevel}
        onPermissionChange={setPermissionLevel}
        saveStatus={saveStatus}
      />
      <div className="flex gap-3" style={{ minHeight: '60vh' }}>
        <div className="flex-[3] overflow-hidden rounded-xl border border-[rgba(175,215,255,0.1)] bg-[rgba(10,18,35,0.4)]">
          <PulseEditorPane markdown={documentMarkdown} onMarkdownChange={setDocumentMarkdown} />
        </div>
        <div className="flex-[2] overflow-hidden rounded-xl border border-[rgba(175,215,255,0.1)] bg-[rgba(10,18,35,0.4)]">
          <PulseChatPane messages={chatHistory} isLoading={isChatLoading} />
          {pendingOps && pendingValidation && (
            <div className="p-3">
              <PulseOpConfirmation
                operations={pendingOps}
                validation={pendingValidation}
                onConfirm={() => {
                  applyOperations(pendingOps)
                  setPendingOps(null)
                  setPendingValidation(null)
                }}
                onReject={() => {
                  setPendingOps(null)
                  setPendingValidation(null)
                }}
              />
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
```

**Step 6: Verify build**

```bash
cd apps/web && pnpm build
```

**Step 7: Commit**

```bash
git add apps/web/components/pulse/
git commit -m "feat(web): pulse workspace — editor, chat, toolbar, autosave, confirmation modal"
```

---

## Task 9: Wire ResultsPanel to render PulseWorkspace

**Files:**
- Modify: `apps/web/components/results-panel.tsx`

**Step 1: Add the workspace branch**

At the top of the `ResultsPanel` component, before any existing rendering logic:

```typescript
import { PulseWorkspace } from '@/components/pulse/pulse-workspace'

// Inside the component body, before existing return:
const { workspaceMode } = useWsMessages()

if (workspaceMode === 'pulse') {
  return <PulseWorkspace />
}
```

This is the only change — the existing results panel stays intact for all CLI command modes.

**Step 2: Verify build**

```bash
cd apps/web && pnpm build
```

**Step 3: Commit**

```bash
git add apps/web/components/results-panel.tsx
git commit -m "feat(web): branch results panel for pulse workspace rendering"
```

---

## Task 10: Vitest configuration + run all tests + final polish

**Files:**
- Check/create: `apps/web/vitest.config.ts`

**Step 1: Ensure vitest is configured**

Check if `vitest.config.ts` exists. If not:

```typescript
import { resolve } from 'node:path'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'node',
    include: ['__tests__/**/*.test.ts'],
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, '.'),
    },
  },
})
```

**Step 2: Run full test suite**

```bash
cd apps/web && pnpm vitest run
```

Expected: ALL tests pass:
- `api-copilot.test.ts` — 3 tests
- `ws-protocol.test.ts` — 7 tests
- `command-map.test.ts` — 2 tests
- `pulse-types.test.ts` — 8 tests
- `pulse-doc-ops.test.ts` — 6 tests
- `pulse-permissions.test.ts` — 4 tests
- `pulse-rag.test.ts` — 3 tests
- `pulse-storage.test.ts` — 5 tests

Total: ~38 tests.

**Step 3: Run Biome lint + format**

```bash
cd apps/web && pnpm lint && pnpm format
```

**Step 4: Full build**

```bash
cd apps/web && pnpm build
```

**Step 5: Manual smoke test**

```bash
# Terminal 1: start axum backend
cd /home/jmagar/workspace/axon_rust && ./scripts/axon serve

# Terminal 2: start Next.js dev
cd apps/web && pnpm dev
```

Verify:
1. Open `http://localhost:3000`
2. Click mode dropdown — `Workspace` category appears with `Pulse`
3. Select `Pulse` — workspace expands (editor + chat pane)
4. Type text in editor — copilot ghost text appears (requires OPENAI env vars)
5. Type a prompt in omnibox — chat response appears in sidebar
6. Permission toggle changes behavior
7. Check `.cache/pulse/` for saved documents

**Step 6: Commit any lint fixes**

```bash
git add -A apps/web/
git commit -m "chore(web): vitest config, lint, format — pulse phase 1 complete"
```

---

## Dependency Graph

```
Task 1 (copilot route + remove faker) ─┐
Task 2 (mode registry + zod) ───────────┤
                                          ├─→ Task 3 (command spec + omnibox routing)
                                          │
                                          ├─→ Task 4 (pulse types) ──→ Task 5 (doc-ops + permissions)
                                          │
                                          ├─→ Task 6 (RAG adapter)
                                          │
                                          └─→ Task 7 (chat + save + storage routes)
                                                     │
Tasks 3, 5, 6, 7 ─────────────────────────→ Task 8 (workspace UI shell)
                                                     │
                                              Task 9 (results panel branch)
                                                     │
                                              Task 10 (tests + polish)
```

**Parallelizable:** Tasks 1+2 (independent foundations), Tasks 4+6 (independent libs), most of 3-7 can run in parallel after 1+2 complete.

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Accidental destructive edits | Threshold guardrails + confirmation modal + local canonical file state |
| Latency from multi-collection retrieval | Bounded top-k per collection + parallel retrieval + context budget (6000 chars) |
| API key/config drift | Strict env validation with actionable 503 errors on every route |
| TEI/Qdrant unavailable | Save always succeeds (filesystem) — embed is best-effort with error logged |
| Large documents overwhelming context | Document markdown truncated to 4000 chars in system prompt |

---

## Definition of Done

- [ ] `Pulse` mode appears in mode picker under `Workspace` category
- [ ] Selecting Pulse expands workspace immediately (editor + chat)
- [ ] Omnibox does NOT send WS execute for pulse prompts
- [ ] Copilot ghost text works with real LLM (faker removed from codebase entirely)
- [ ] Chat route returns RAG-grounded responses with citations
- [ ] Safe ops auto-apply; high-risk ops require confirmation
- [ ] Documents save to filesystem with frontmatter
- [ ] Saved docs auto-embed into Qdrant `pulse` collection
- [ ] Permission toggle (plan/training-wheels/full-access) changes behavior
- [ ] 38+ tests passing across all pulse modules
- [ ] `pnpm build` clean, `pnpm lint` clean
