'use client'

import dynamic from 'next/dynamic'

export const EditorPane = dynamic(
  () => import('@/components/editor/editor-pane').then((m) => ({ default: m.PulseEditorPane })),
  {
    ssr: false,
    loading: () => (
      <div className="flex h-full w-full flex-col">
        <div className="h-12 w-full border-b border-[rgba(175,215,255,0.08)] bg-[linear-gradient(180deg,rgba(10,18,35,0.64),rgba(4,9,20,0.68))]" />
        <div className="flex-1 bg-transparent" />
      </div>
    ),
  },
)
