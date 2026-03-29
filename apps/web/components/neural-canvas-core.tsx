'use client'

import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from 'react'
import {
  DEFAULT_NEURAL_CANVAS_PROFILE,
  getNeuralCanvasPreset,
  type NeuralCanvasProfile,
} from '@/lib/pulse/neural-canvas-presets'
import type { ContainerStats } from '@/lib/ws-protocol'

// Re-export public API types
export type { NeuralCanvasHandle, NeuralCanvasProps } from './neural-canvas/types'

import {
  createAnimState,
  createBackgroundLayer,
  createDensityLayer,
} from './neural-canvas/anim-state'
// Internal imports
import { applyPalette, rgba } from './neural-canvas/color-utils'
import { ActionPotential, drawConnections } from './neural-canvas/synapse'
import type { AnimState, NeuralCanvasHandle, NeuralCanvasProps } from './neural-canvas/types'

// ---------------------------------------------------------------------------
// Hook: useNeuralCanvasProfile
// ---------------------------------------------------------------------------

const STORAGE_KEY = 'axon.web.neural-canvas.profile'

export function useNeuralCanvasProfile() {
  const [profile, setProfile] = useState<NeuralCanvasProfile>(() => {
    if (typeof window === 'undefined') return DEFAULT_NEURAL_CANVAS_PROFILE
    return (
      (localStorage.getItem(STORAGE_KEY) as NeuralCanvasProfile) ?? DEFAULT_NEURAL_CANVAS_PROFILE
    )
  })

  const changeProfile = useCallback((p: NeuralCanvasProfile) => {
    setProfile(p)
    localStorage.setItem(STORAGE_KEY, p)
  }, [])

  return { profile, changeProfile }
}

// ---------------------------------------------------------------------------
// React component
// ---------------------------------------------------------------------------

const NeuralCanvas = forwardRef<NeuralCanvasHandle, NeuralCanvasProps>(function NeuralCanvas(
  { profile = DEFAULT_NEURAL_CANVAS_PROFILE },
  ref,
) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const stateRef = useRef<AnimState | null>(null)

  useImperativeHandle(ref, () => ({
    setIntensity(target: number) {
      if (stateRef.current) {
        stateRef.current.targetIntensity = Math.max(0, Math.min(1, target))
      }
    },
    stimulate(containers: Record<string, ContainerStats>) {
      if (!stateRef.current) return
      const values = Object.values(containers)
      if (values.length === 0) return
      const avgCpu = values.reduce((sum, c) => sum + c.cpu_percent, 0) / values.length
      stateRef.current.targetIntensity = Math.max(0, Math.min(1, avgCpu / 100))
      const state = stateRef.current
      const stimCount = Math.floor(avgCpu / 20)
      for (let i = 0; i < stimCount && i < state.neurons.length; i++) {
        const idx = Math.floor(Math.random() * state.neurons.length)
        // idx is bounded by state.neurons.length — always a valid index
        state.neurons[idx]!.receiveSignal(20 + avgCpu * 0.3)
      }
    },
  }))

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return

    const ctx = canvas.getContext('2d')
    if (!ctx) return
    const visual = getNeuralCanvasPreset(profile)
    applyPalette(visual)

    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    const hardwareCores =
      typeof navigator.hardwareConcurrency === 'number' ? navigator.hardwareConcurrency : 8
    const deviceMemory =
      typeof (navigator as Navigator & { deviceMemory?: number }).deviceMemory === 'number'
        ? ((navigator as Navigator & { deviceMemory?: number }).deviceMemory ?? 8)
        : 8
    const lowCoreCount = hardwareCores <= 4
    const megapixels = (window.innerWidth * window.innerHeight) / 1_000_000
    const performanceScore = Math.max(
      0.55,
      Math.min(1.25, hardwareCores / 8) * Math.max(0.7, Math.min(1.2, deviceMemory / 8)),
    )
    const pixelPenalty = megapixels > 3 ? 0.82 : megapixels > 2 ? 0.9 : 1
    const qualityScale = Math.max(
      0.52,
      Math.min(1.1, (reducedMotion ? 0.72 : 1) * performanceScore * pixelPenalty),
    )

    const maxDpr =
      reducedMotion || lowCoreCount || megapixels > 3 ? 1 : megapixels > 2 ? 1.15 : 1.25
    const targetFrameMs = reducedMotion
      ? 1000 / 35
      : qualityScale < 0.75
        ? 1000 / 48
        : lowCoreCount
          ? 1000 / 52
          : 1000 / 60
    const viewportScale = Math.min(
      1.15,
      Math.max(0.7, (window.innerWidth * window.innerHeight) / (1920 * 1080)),
    )
    const neuronCount = Math.max(
      26,
      Math.round((reducedMotion ? 42 : 62) * viewportScale * qualityScale),
    )
    const particleCount = Math.max(
      110,
      Math.round((reducedMotion ? 180 : 320) * viewportScale * qualityScale),
    )

    // Size to viewport with DPR scaling
    const resize = () => {
      const dpr = Math.min(window.devicePixelRatio || 1, maxDpr)
      const w = window.innerWidth
      const h = window.innerHeight
      canvas.width = w * dpr
      canvas.height = h * dpr
      canvas.style.width = `${w}px`
      canvas.style.height = `${h}px`
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      if (stateRef.current) {
        stateRef.current.width = w
        stateRef.current.height = h
        stateRef.current.densityLayer = createDensityLayer(w, h, reducedMotion, visual)
        stateRef.current.backgroundLayer = createBackgroundLayer(w, h)
        stateRef.current.backgroundNeedsRefresh = true
      }
    }
    resize()

    // Debounced resize
    let resizeTimeout: ReturnType<typeof setTimeout>
    const onResize = () => {
      clearTimeout(resizeTimeout)
      resizeTimeout = setTimeout(resize, 100)
    }
    window.addEventListener('resize', onResize)

    // Initialize state
    const state = createAnimState(window.innerWidth, window.innerHeight, {
      neuronCount,
      particleCount,
      targetFrameMs,
      reducedMotion,
      visual,
    })
    state.isVisible = document.visibilityState === 'visible'
    stateRef.current = state

    // Track tab visibility
    const onVisibilityChange = () => {
      if (!stateRef.current) return
      stateRef.current.isVisible = document.visibilityState === 'visible' && !canvasOccluded
    }
    document.addEventListener('visibilitychange', onVisibilityChange)

    // Occlusion detection — pause animation when canvas is fully covered by opaque overlay
    let canvasOccluded = false
    const intersectionObserver = new IntersectionObserver(
      ([entry]) => {
        if (!entry || !stateRef.current) return
        canvasOccluded = entry.intersectionRatio < 0.01
        stateRef.current.isVisible = document.visibilityState === 'visible' && !canvasOccluded
      },
      { threshold: [0, 0.01, 0.1] },
    )
    intersectionObserver.observe(canvas)

    // Animation loop
    const animate = (currentTime: number) => {
      if (!state.isVisible) {
        state.lastTime = currentTime
        state.lastRenderTime = currentTime
        state.frameId = requestAnimationFrame(animate)
        return
      }

      const dt = Math.min(currentTime - state.lastTime, 50)
      state.lastTime = currentTime
      if (currentTime - state.lastRenderTime < state.targetFrameMs) {
        state.frameId = requestAnimationFrame(animate)
        return
      }
      state.lastRenderTime = currentTime
      state.frameCount++

      const fps = dt > 0 ? 1000 / dt : 60
      state.fpsEma = state.fpsEma * 0.9 + fps * 0.1
      const degraded = reducedMotion || state.fpsEma < 44
      const medium = state.fpsEma < 52
      const particleStride = degraded ? 3 : medium ? 2 : 1
      const connectionStride = degraded ? 3 : medium ? 2 : 1
      const signalCap = degraded ? 36 : medium ? 48 : 60
      const fineNeuronBudget = degraded ? 0 : medium ? 10 : 18
      const spineNeuronBudget = degraded ? 0 : medium ? 8 : 16

      // Smooth intensity response
      const intensityDelta = state.targetIntensity - state.intensity
      const lerpRate =
        intensityDelta > 0
          ? 0.12
          : 0.008 +
            Math.min(1, Math.abs(intensityDelta)) ** 2 *
              (0.02 / Math.max(0.5, state.visual.calmRecovery))
      state.intensity += (state.targetIntensity - state.intensity) * lerpRate
      state.burstCooldown = Math.max(0, state.burstCooldown - dt)

      const w = state.width
      const h = state.height

      // Clear frame
      ctx.clearRect(0, 0, w, h)
      ctx.fillStyle = rgba(state.visual.palette.backgroundOuter, 1)
      ctx.fillRect(0, 0, w, h)

      // Update particles
      state.particles.forEach((p) => {
        const parallax = 0.9 + p.z * 0.8 * state.visual.parallaxDepth
        p.update(currentTime, w, h, parallax)
      })

      // Layered background refresh every N frames
      if (state.backgroundNeedsRefresh || state.frameCount % state.backgroundInterval === 0) {
        const bgCtx = state.backgroundLayer.getContext('2d')
        if (bgCtx) {
          bgCtx.clearRect(0, 0, w, h)
          bgCtx.fillStyle = rgba(state.visual.palette.backgroundOuter, 1)
          bgCtx.fillRect(0, 0, w, h)
          bgCtx.drawImage(state.densityLayer, 0, 0, w, h)

          const cx = w / 2
          const cy = h / 2
          const ambR = Math.max(w, h) * 0.5
          const ambG = bgCtx.createRadialGradient(cx, cy, 0, cx, cy, ambR)
          ambG.addColorStop(
            0,
            rgba(
              state.visual.palette.backgroundInner,
              (0.05 + state.intensity * 0.035) * state.visual.brightness,
            ),
          )
          ambG.addColorStop(0.5, rgba(state.visual.palette.dim, 0.02 * state.visual.brightness))
          ambG.addColorStop(1, rgba(state.visual.palette.backgroundOuter, 0))
          bgCtx.fillStyle = ambG
          bgCtx.fillRect(0, 0, w, h)

          state.particles.forEach((p, index) => {
            if (index % particleStride !== 0) return
            if (p.z < 0.5) {
              p.draw(bgCtx, currentTime, state.renderAssets, !degraded, degraded)
            }
          })
        }
        state.backgroundNeedsRefresh = false
      }
      ctx.drawImage(state.backgroundLayer, 0, 0, w, h)

      const masterAlpha = 0.94 + state.intensity * 0.08 * state.visual.brightness
      ctx.save()
      ctx.globalAlpha = masterAlpha

      // Connection fibers
      drawConnections(
        ctx,
        state.connectionBuckets,
        state.intensity,
        currentTime,
        state.visual,
        connectionStride,
      )

      // Update and draw neurons (sorted back-to-front)
      let fineUsed = 0
      let spineUsed = 0
      state.neurons.forEach((neuron) => {
        const neuronParallax = 0.8 + neuron.depth * 0.9 * state.visual.parallaxDepth
        neuron.update(currentTime, dt, w, h, neuronParallax)
        const allowFineDetail = !degraded && neuron.depth > 0.45 && fineUsed < fineNeuronBudget
        const allowSpines = !degraded && neuron.depth > 0.35 && spineUsed < spineNeuronBudget
        neuron.draw(ctx, currentTime, {
          fineDetail: allowFineDetail,
          allowSpines,
          assets: state.renderAssets,
          visual: state.visual,
        })
        if (allowFineDetail) fineUsed++
        if (allowSpines) spineUsed++

        // Fire action potentials
        if (neuron.isFiring && neuron.firePhase === 1 && neuron.fireTimer < dt * 2) {
          const outgoing = neuron.outgoingConnections
          const maxSignals = Math.max(1, 2 + Math.floor((state.intensity * 5) / connectionStride))
          let sent = 0
          for (let c = 0; c < outgoing.length && sent < maxSignals; c++) {
            // c is bounded by outgoing.length — always a valid index
            if (Math.random() < outgoing[c]!.strength * 0.4) {
              state.signals.push(new ActionPotential(neuron as never, outgoing[c]! as never))
              sent++
            }
          }
        }

        // Intensity-driven spontaneous activity
        if (state.intensity > 0.1 && !neuron.isFiring && neuron.refractoryTime <= 0) {
          if (Math.random() < state.intensity * 0.025 * state.visual.activity) {
            neuron.epsp += 25
          }
        }
      })

      if (
        state.intensity >= state.visual.burstThreshold &&
        state.burstCooldown <= 0 &&
        state.neurons.length > 0
      ) {
        const focusIndex = Math.floor(Math.random() * state.neurons.length)
        // focusIndex is bounded by state.neurons.length (length > 0 checked above)
        const focus = state.neurons[focusIndex]!
        const burstRadiusSq = (180 * state.visual.burstStrength) ** 2
        let excited = 0
        for (let i = 0; i < state.neurons.length && excited < 16; i++) {
          // i is bounded by state.neurons.length — always a valid index
          const n = state.neurons[i]!
          const dx = n.x - focus.x
          const dy = n.y - focus.y
          const near = dx * dx + dy * dy < burstRadiusSq
          if (near && Math.random() < 0.55 * state.visual.burstStrength) {
            n.receiveSignal(22 * state.visual.burstStrength)
            excited++
          }
        }
        state.burstCooldown =
          Math.max(260, 980 - state.intensity * 500) / Math.max(0.85, state.visual.burstStrength)
      }

      // Action potentials
      for (let i = state.signals.length - 1; i >= 0; i--) {
        // i is a valid index — loop iterates from length-1 down to 0
        state.signals[i]!.update(dt)
        state.signals[i]!.draw(ctx, state.renderAssets, !degraded)
        if (!state.signals[i]!.active) state.signals.splice(i, 1)
      }
      while (state.signals.length > signalCap) state.signals.shift()

      // Foreground particles (z >= 0.5)
      state.particles.forEach((p, index) => {
        if (index % particleStride !== 0) return
        if (p.z >= 0.5) p.draw(ctx, currentTime, state.renderAssets, !degraded, degraded)
      })
      ctx.restore()

      state.frameId = requestAnimationFrame(animate)
    }

    state.frameId = requestAnimationFrame(animate)

    // Cleanup
    return () => {
      cancelAnimationFrame(state.frameId)
      window.removeEventListener('resize', onResize)
      document.removeEventListener('visibilitychange', onVisibilityChange)
      intersectionObserver.disconnect()
      clearTimeout(resizeTimeout)
      stateRef.current = null
    }
  }, [profile])

  return (
    <canvas ref={canvasRef} className="fixed inset-0 z-0" style={{ willChange: 'transform' }} />
  )
})

NeuralCanvas.displayName = 'NeuralCanvas'

export { NeuralCanvas }
