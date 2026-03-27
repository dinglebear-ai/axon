'use client'

import { useMemo } from 'react'
import { useShellStore } from '@/lib/shell-store'

export function useAxonShellSettings() {
  const enableFs = useShellStore((s) => s.enableFs)
  const enableTerminal = useShellStore((s) => s.enableTerminal)
  const permissionTimeoutSecs = useShellStore((s) => s.permissionTimeoutSecs)
  const adapterTimeoutSecs = useShellStore((s) => s.adapterTimeoutSecs)
  const setEnableFs = useShellStore((s) => s.setEnableFs)
  const setEnableTerminal = useShellStore((s) => s.setEnableTerminal)
  const setPermissionTimeoutSecs = useShellStore((s) => s.setPermissionTimeoutSecs)
  const setAdapterTimeoutSecs = useShellStore((s) => s.setAdapterTimeoutSecs)

  const settingsState = useMemo(
    () => ({
      enableFs,
      setEnableFs,
      enableTerminal,
      setEnableTerminal,
      permissionTimeoutSecs,
      setPermissionTimeoutSecs,
      adapterTimeoutSecs,
      setAdapterTimeoutSecs,
    }),
    [
      enableFs,
      setEnableFs,
      enableTerminal,
      setEnableTerminal,
      permissionTimeoutSecs,
      setPermissionTimeoutSecs,
      adapterTimeoutSecs,
      setAdapterTimeoutSecs,
    ],
  )

  return {
    enableFs,
    setEnableFs,
    enableTerminal,
    setEnableTerminal,
    permissionTimeoutSecs,
    setPermissionTimeoutSecs,
    adapterTimeoutSecs,
    setAdapterTimeoutSecs,
    settingsState,
  }
}
