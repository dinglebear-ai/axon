'use client'

import type { ComponentProps, HTMLAttributes } from 'react'
import { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

type ConfirmationContextValue = {
  open: boolean
  setOpen: (value: boolean) => void
}

const ConfirmationContext = createContext<ConfirmationContextValue | null>(null)

function useConfirmationContext() {
  const context = useContext(ConfirmationContext)
  if (!context) {
    throw new Error('Confirmation components must be used within Confirmation')
  }
  return context
}

export function Confirmation({ className, children }: HTMLAttributes<HTMLDivElement>) {
  const [open, setOpen] = useState(false)

  return (
    <ConfirmationContext.Provider value={{ open, setOpen }}>
      <div className={cn('relative', className)}>{children}</div>
    </ConfirmationContext.Provider>
  )
}

export function ConfirmationTrigger({ onClick, ...props }: ComponentProps<typeof Button>) {
  const { setOpen } = useConfirmationContext()
  return (
    <Button
      type="button"
      {...props}
      onClick={(event) => {
        onClick?.(event)
        setOpen(true)
      }}
    />
  )
}

export function ConfirmationContent({
  className,
  children,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  const { open, setOpen } = useConfirmationContext()
  const panelRef = useRef<HTMLDivElement>(null)

  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpen(false)
      }
      if (event.key === 'Tab' && panelRef.current) {
        const focusable = panelRef.current.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        )
        if (focusable.length === 0) return
        const first = focusable[0]!
        const last = focusable[focusable.length - 1]!
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault()
          last.focus()
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault()
          first.focus()
        }
      }
    },
    [setOpen],
  )

  useEffect(() => {
    if (!open) return
    document.addEventListener('keydown', handleKeyDown)
    const timer = setTimeout(() => {
      const firstButton = panelRef.current?.querySelector<HTMLElement>('button')
      firstButton?.focus()
    }, 0)
    return () => {
      document.removeEventListener('keydown', handleKeyDown)
      clearTimeout(timer)
    }
  }, [open, handleKeyDown])

  if (!open) return null

  return (
    <>
      <button
        type="button"
        className="fixed inset-0 z-10 h-full w-full cursor-default bg-transparent"
        onClick={() => setOpen(false)}
        aria-label="Close confirmation"
        tabIndex={-1}
      />
      <div
        ref={panelRef}
        role="alertdialog"
        aria-modal="true"
        className={cn(
          'absolute right-0 top-full z-20 mt-2 w-72 rounded-2xl border border-[var(--border-subtle)] bg-[var(--glass-overlay)] p-3 shadow-[var(--shadow-lg)]',
          className,
        )}
        {...props}
      >
        {children}
      </div>
    </>
  )
}

export function ConfirmationCancel({ onClick, ...props }: ComponentProps<typeof Button>) {
  const { setOpen } = useConfirmationContext()
  return (
    <Button
      type="button"
      variant="ghost"
      {...props}
      onClick={(event) => {
        onClick?.(event)
        setOpen(false)
      }}
    />
  )
}

export function ConfirmationAction({ onClick, ...props }: ComponentProps<typeof Button>) {
  const { setOpen } = useConfirmationContext()
  return (
    <Button
      type="button"
      {...props}
      onClick={(event) => {
        onClick?.(event)
        setOpen(false)
      }}
    />
  )
}
