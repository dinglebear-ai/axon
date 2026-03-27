import { useCallback, useEffect, useMemo, useRef } from 'react'
import type { NeuralCanvasProfile } from '@/lib/pulse/neural-canvas-presets'
import { useShellStore } from '@/lib/shell-store'
import { getStorageItem, removeStorageItem, setStorageItem } from '@/lib/storage'
import {
  AXON_MOBILE_PANE_STORAGE_KEY,
  type AxonDensity,
  type AxonMobilePane,
  CANVAS_PROFILE_STORAGE_KEY,
  CANVAS_PROFILES,
  CHAT_FLEX_STORAGE_KEY,
  CHAT_OPEN_STORAGE_KEY,
  DENSITY_STORAGE_KEY,
  PANE_WIDTH_MIN,
  RAIL_MODE_STORAGE_KEY,
  RIGHT_PANE_STORAGE_KEY,
  type RightPane,
  readStoredBool,
  readStoredDensity,
  readStoredFloat,
  readStoredRailMode,
  SIDEBAR_OPEN_STORAGE_KEY,
  SIDEBAR_WIDTH_DEFAULT,
  SIDEBAR_WIDTH_MAX,
  SIDEBAR_WIDTH_MIN,
  SIDEBAR_WIDTH_STORAGE_KEY,
  VALID_RIGHT_PANES,
} from './axon-shell-state-helpers'
import type { RailMode } from './axon-ui-config'

type LayoutStateFields = {
  canvasProfile: NeuralCanvasProfile
  chatFlex: number
  chatOpen: boolean
  density: AxonDensity
  editorOpen: boolean
  isDragging: boolean
  layoutRestored: boolean
  mobilePane: AxonMobilePane
  railMode: RailMode
  rightPane: RightPane
  sectionRef: React.RefObject<HTMLElement | null>
  sidebarOpen: boolean
  sidebarWidth: number
  transitionClass: string
}

type LayoutActionFields = {
  handleCanvasProfileChange: (profile: NeuralCanvasProfile) => void
  nudgeChatFlex: (delta: number) => void
  nudgeSidebar: (delta: number) => void
  persistChatOpen: (open: boolean) => void
  persistRightPane: (pane: RightPane) => void
  persistSidebarOpen: (open: boolean) => void
  resetChatFlex: () => void
  resetSidebarWidth: () => void
  setDensityTracked: (density: AxonDensity) => void
  setMobilePaneTracked: (nextPane: AxonMobilePane) => void
  setRailModeTracked: (mode: RailMode) => void
  startChatResize: (startX: number) => void
  startSidebarResize: (startX: number) => void
}

type LayoutControls = LayoutStateFields &
  LayoutActionFields & {
    layoutState: LayoutStateFields
    layoutActions: LayoutActionFields
  }

export function useAxonShellLayoutControls(): LayoutControls {
  // Read from store (surgical per-field subscriptions)
  const railMode = useShellStore((s) => s.railMode)
  const mobilePane = useShellStore((s) => s.mobilePane)
  const sidebarOpen = useShellStore((s) => s.sidebarOpen)
  const chatOpen = useShellStore((s) => s.chatOpen)
  const rightPane = useShellStore((s) => s.rightPane)
  const density = useShellStore((s) => s.density)
  const canvasProfile = useShellStore((s) => s.canvasProfile)
  const sidebarWidth = useShellStore((s) => s.sidebarWidth)
  const chatFlex = useShellStore((s) => s.chatFlex)
  const isDragging = useShellStore((s) => s.isDragging)
  const layoutRestored = useShellStore((s) => s.layoutRestored)

  // Write actions from store
  const setRailMode = useShellStore((s) => s.setRailMode)
  const setMobilePane = useShellStore((s) => s.setMobilePane)
  const setSidebarOpen = useShellStore((s) => s.setSidebarOpen)
  const setChatOpen = useShellStore((s) => s.setChatOpen)
  const setRightPane = useShellStore((s) => s.setRightPane)
  const setDensity = useShellStore((s) => s.setDensity)
  const setCanvasProfile = useShellStore((s) => s.setCanvasProfile)
  const setSidebarWidth = useShellStore((s) => s.setSidebarWidth)
  const setChatFlex = useShellStore((s) => s.setChatFlex)
  const setIsDragging = useShellStore((s) => s.setIsDragging)
  const setLayoutRestored = useShellStore((s) => s.setLayoutRestored)

  const editorOpen = rightPane !== null
  const sectionRef = useRef<HTMLElement>(null)

  // Restore persisted layout once on mount
  useEffect(() => {
    const saved = getStorageItem(AXON_MOBILE_PANE_STORAGE_KEY)
    if (
      saved === 'sidebar' ||
      saved === 'chat' ||
      saved === 'editor' ||
      saved === 'terminal' ||
      saved === 'logs' ||
      saved === 'mcp' ||
      saved === 'settings' ||
      saved === 'cortex'
    ) {
      setMobilePane(saved as AxonMobilePane)
    }
    setSidebarWidth(
      readStoredFloat(
        SIDEBAR_WIDTH_STORAGE_KEY,
        SIDEBAR_WIDTH_DEFAULT,
        SIDEBAR_WIDTH_MIN,
        SIDEBAR_WIDTH_MAX,
      ),
    )
    setChatFlex(readStoredFloat(CHAT_FLEX_STORAGE_KEY, 1))
    setSidebarOpen(readStoredBool(SIDEBAR_OPEN_STORAGE_KEY, true))
    setChatOpen(readStoredBool(CHAT_OPEN_STORAGE_KEY, true))
    const storedPane = getStorageItem(RIGHT_PANE_STORAGE_KEY)
    if (storedPane === '') {
      setRightPane(null)
    } else if (storedPane && VALID_RIGHT_PANES.has(storedPane)) {
      setRightPane(storedPane as RightPane)
    } else {
      setRightPane('editor')
    }
    setRailMode(readStoredRailMode(RAIL_MODE_STORAGE_KEY, 'sessions'))
    setDensity(readStoredDensity(DENSITY_STORAGE_KEY, 'high'))
    const rawProfile = getStorageItem(CANVAS_PROFILE_STORAGE_KEY)
    if (rawProfile && CANVAS_PROFILES.includes(rawProfile as NeuralCanvasProfile)) {
      setCanvasProfile(rawProfile as NeuralCanvasProfile)
    }
    setLayoutRestored(true)
    // Run once on mount — store setters are stable references, no need in deps
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    setCanvasProfile,
    setChatFlex,
    setChatOpen,
    setDensity,
    setLayoutRestored,
    setMobilePane,
    setRailMode,
    setRightPane,
    setSidebarOpen,
    setSidebarWidth,
  ])

  const setRailModeTracked = useCallback(
    (mode: RailMode) => {
      setRailMode(mode)
      setStorageItem(RAIL_MODE_STORAGE_KEY, mode)
    },
    [setRailMode],
  )

  const setDensityTracked = useCallback(
    (next: AxonDensity) => {
      setDensity(next)
      setStorageItem(DENSITY_STORAGE_KEY, next)
    },
    [setDensity],
  )

  const setMobilePaneTracked = useCallback(
    (nextPane: AxonMobilePane) => {
      setMobilePane(nextPane)
      setStorageItem(AXON_MOBILE_PANE_STORAGE_KEY, nextPane)
    },
    [setMobilePane],
  )

  const handleCanvasProfileChange = useCallback(
    (profile: NeuralCanvasProfile) => {
      setCanvasProfile(profile)
      setStorageItem(CANVAS_PROFILE_STORAGE_KEY, profile)
    },
    [setCanvasProfile],
  )

  const persistSidebarOpen = useCallback(
    (open: boolean) => {
      if (!open && !chatOpen && rightPane === null) return
      setSidebarOpen(open)
      setStorageItem(SIDEBAR_OPEN_STORAGE_KEY, String(open))
    },
    [chatOpen, rightPane, setSidebarOpen],
  )

  const persistChatOpen = useCallback(
    (open: boolean) => {
      if (!open && !sidebarOpen && rightPane === null) return
      setChatOpen(open)
      setStorageItem(CHAT_OPEN_STORAGE_KEY, String(open))
    },
    [sidebarOpen, rightPane, setChatOpen],
  )

  const persistRightPane = useCallback(
    (pane: RightPane) => {
      if (pane === null && !sidebarOpen && !chatOpen) return
      setRightPane(pane)
      setStorageItem(RIGHT_PANE_STORAGE_KEY, pane ?? '')
    },
    [chatOpen, sidebarOpen, setRightPane],
  )

  const startSidebarResize = useCallback(
    (startX: number) => {
      const initWidth = sidebarWidth
      let lastWidth = initWidth
      setIsDragging(true)
      const onMove = (e: MouseEvent) => {
        lastWidth = Math.max(
          SIDEBAR_WIDTH_MIN,
          Math.min(SIDEBAR_WIDTH_MAX, initWidth + e.clientX - startX),
        )
        setSidebarWidth(lastWidth)
      }
      const onUp = () => {
        document.removeEventListener('mousemove', onMove)
        document.removeEventListener('mouseup', onUp)
        document.body.style.removeProperty('cursor')
        document.body.style.removeProperty('user-select')
        setIsDragging(false)
        setStorageItem(SIDEBAR_WIDTH_STORAGE_KEY, String(lastWidth))
      }
      document.body.style.cursor = 'col-resize'
      document.body.style.userSelect = 'none'
      document.addEventListener('mousemove', onMove)
      document.addEventListener('mouseup', onUp)
    },
    [sidebarWidth, setIsDragging, setSidebarWidth],
  )

  const resetSidebarWidth = useCallback(() => {
    setSidebarWidth(SIDEBAR_WIDTH_DEFAULT)
    removeStorageItem(SIDEBAR_WIDTH_STORAGE_KEY)
  }, [setSidebarWidth])

  const startChatResize = useCallback(
    (startX: number) => {
      const section = sectionRef.current
      if (!section) return
      const sidebarPx = sidebarOpen ? sidebarWidth : 40
      const available = section.offsetWidth - sidebarPx
      const totalFlex = chatFlex + 1
      const initChatPx = (available * chatFlex) / totalFlex
      let lastFlex = chatFlex
      setIsDragging(true)
      const onMove = (e: MouseEvent) => {
        const newChatPx = Math.max(
          PANE_WIDTH_MIN,
          Math.min(available - PANE_WIDTH_MIN, initChatPx + e.clientX - startX),
        )
        lastFlex = newChatPx / (available - newChatPx)
        setChatFlex(lastFlex)
      }
      const onUp = () => {
        document.removeEventListener('mousemove', onMove)
        document.removeEventListener('mouseup', onUp)
        document.body.style.removeProperty('cursor')
        document.body.style.removeProperty('user-select')
        setIsDragging(false)
        setStorageItem(CHAT_FLEX_STORAGE_KEY, String(lastFlex))
      }
      document.body.style.cursor = 'col-resize'
      document.body.style.userSelect = 'none'
      document.addEventListener('mousemove', onMove)
      document.addEventListener('mouseup', onUp)
    },
    [chatFlex, sidebarOpen, sidebarWidth, setChatFlex, setIsDragging],
  )

  const resetChatFlex = useCallback(() => {
    setChatFlex(1)
    removeStorageItem(CHAT_FLEX_STORAGE_KEY)
  }, [setChatFlex])

  const nudgeSidebar = useCallback(
    (delta: number) => {
      setSidebarWidth((w) => {
        const next = Math.max(SIDEBAR_WIDTH_MIN, Math.min(SIDEBAR_WIDTH_MAX, w + delta))
        setStorageItem(SIDEBAR_WIDTH_STORAGE_KEY, String(next))
        return next
      })
    },
    [setSidebarWidth],
  )

  const nudgeChatFlex = useCallback(
    (delta: number) => {
      const section = sectionRef.current
      if (!section) return
      const sidebarPx = sidebarOpen ? sidebarWidth : 40
      const available = section.offsetWidth - sidebarPx
      setChatFlex((f) => {
        const currentChatPx = (available * f) / (f + 1)
        const newChatPx = Math.max(
          PANE_WIDTH_MIN,
          Math.min(available - PANE_WIDTH_MIN, currentChatPx + delta),
        )
        const next = newChatPx / (available - newChatPx)
        setStorageItem(CHAT_FLEX_STORAGE_KEY, String(next))
        return next
      })
    },
    [sidebarOpen, sidebarWidth, setChatFlex],
  )

  const transitionClass = useMemo(
    () => (isDragging || !layoutRestored ? '' : 'transition-[width,flex] duration-300 ease-out'),
    [isDragging, layoutRestored],
  )

  const layoutState = useMemo(
    () => ({
      canvasProfile,
      chatFlex,
      chatOpen,
      editorOpen,
      isDragging,
      layoutRestored,
      mobilePane,
      railMode,
      rightPane,
      sectionRef,
      sidebarOpen,
      sidebarWidth,
      transitionClass,
      density,
    }),
    [
      canvasProfile,
      chatFlex,
      chatOpen,
      editorOpen,
      isDragging,
      layoutRestored,
      mobilePane,
      railMode,
      rightPane,
      sectionRef,
      sidebarOpen,
      sidebarWidth,
      transitionClass,
      density,
    ],
  )

  const layoutActions = useMemo(
    () => ({
      handleCanvasProfileChange,
      nudgeChatFlex,
      nudgeSidebar,
      persistChatOpen,
      persistRightPane,
      persistSidebarOpen,
      resetChatFlex,
      resetSidebarWidth,
      setMobilePaneTracked,
      setRailModeTracked,
      startChatResize,
      startSidebarResize,
      setDensityTracked,
    }),
    [
      handleCanvasProfileChange,
      nudgeChatFlex,
      nudgeSidebar,
      persistChatOpen,
      persistRightPane,
      persistSidebarOpen,
      resetChatFlex,
      resetSidebarWidth,
      setMobilePaneTracked,
      setRailModeTracked,
      startChatResize,
      startSidebarResize,
      setDensityTracked,
    ],
  )

  return {
    canvasProfile,
    chatFlex,
    chatOpen,
    editorOpen,
    handleCanvasProfileChange,
    isDragging,
    layoutActions,
    layoutRestored,
    layoutState,
    mobilePane,
    nudgeChatFlex,
    nudgeSidebar,
    persistChatOpen,
    persistRightPane,
    persistSidebarOpen,
    railMode,
    resetChatFlex,
    resetSidebarWidth,
    rightPane,
    sectionRef,
    setMobilePaneTracked,
    setRailModeTracked,
    sidebarOpen,
    sidebarWidth,
    startChatResize,
    startSidebarResize,
    transitionClass,
    density,
    setDensityTracked,
  }
}
