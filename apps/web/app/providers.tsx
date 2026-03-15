'use client'

import type { ReactNode } from 'react'
import { Toaster } from 'sonner'
import { AppShell } from '@/components/app-shell'
import { TooltipProvider } from '@/components/ui/tooltip'
import { AxonWsContext, useAxonWsProvider } from '@/hooks/use-axon-ws'
import {
  useWsMessagesProvider,
  WsMessagesActionsContext,
  WsMessagesContext,
  WsMessagesExecutionContext,
  WsMessagesWorkspaceContext,
} from '@/hooks/use-ws-messages'

export function Providers({ children }: { children: ReactNode }) {
  const ws = useAxonWsProvider()
  return (
    <AxonWsContext value={ws}>
      <WsMessagesProvider>{children}</WsMessagesProvider>
    </AxonWsContext>
  )
}

function WsMessagesProvider({ children }: { children: ReactNode }) {
  const messages = useWsMessagesProvider()
  return (
    <WsMessagesExecutionContext value={messages.executionState}>
      <WsMessagesWorkspaceContext value={messages.workspaceState}>
        <WsMessagesActionsContext value={messages.actions}>
          <WsMessagesContext value={messages.value}>
            <TooltipProvider>
              <AppShell>{children}</AppShell>
              <Toaster theme="dark" position="bottom-right" />
            </TooltipProvider>
          </WsMessagesContext>
        </WsMessagesActionsContext>
      </WsMessagesWorkspaceContext>
    </WsMessagesExecutionContext>
  )
}
