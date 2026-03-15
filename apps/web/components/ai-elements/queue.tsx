'use client'

import { ChevronDownIcon, PaperclipIcon } from 'lucide-react'
import Image from 'next/image'
import type React from 'react'
import type { ComponentProps } from 'react'
import { Button } from '@/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { ScrollArea } from '@/components/ui/scroll-area'
import { cn } from '@/lib/utils'

export function Queue({ className, ...props }: ComponentProps<'div'>) {
  return (
    <div
      className={cn(
        'flex flex-col gap-2 rounded-3xl border border-[rgba(135,175,255,0.12)] bg-[rgba(7,12,26,0.72)] px-3 pb-2 pt-2 shadow-xs',
        className,
      )}
      {...props}
    />
  )
}

export function QueueSection({
  className,
  defaultOpen = true,
  ...props
}: ComponentProps<typeof Collapsible>) {
  return <Collapsible className={className} defaultOpen={defaultOpen} {...props} />
}

export function QueueSectionTrigger({ children, className, ...props }: ComponentProps<'button'>) {
  return (
    <CollapsibleTrigger asChild>
      <button
        className={cn(
          'group flex w-full items-center justify-between rounded-2xl bg-[rgba(255,255,255,0.04)] px-3 py-2 text-left text-sm font-medium text-muted-foreground transition-colors hover:bg-[rgba(255,255,255,0.08)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring-color)] focus-visible:ring-offset-0',
          className,
        )}
        type="button"
        {...props}
      >
        {children}
      </button>
    </CollapsibleTrigger>
  )
}

export function QueueSectionLabel({
  count,
  label,
  icon,
  className,
  ...props
}: ComponentProps<'span'> & { count?: number; label: string; icon?: React.ReactNode }) {
  return (
    <span className={cn('flex items-center gap-2', className)} {...props}>
      <ChevronDownIcon className="size-4 transition-transform group-data-[state=closed]:-rotate-90" />
      {icon}
      <span>
        {count ? `${count} ` : ''}
        {label}
      </span>
    </span>
  )
}

export function QueueSectionContent({
  className,
  ...props
}: ComponentProps<typeof CollapsibleContent>) {
  return <CollapsibleContent className={className} {...props} />
}

export function QueueList({ children, className, ...props }: ComponentProps<typeof ScrollArea>) {
  return (
    <ScrollArea className={cn('-mb-1 mt-2 min-h-0 flex-1', className)} {...props}>
      <div className="pr-4">
        <ul>{children}</ul>
      </div>
    </ScrollArea>
  )
}

export function QueueItem({ className, ...props }: ComponentProps<'li'>) {
  return (
    <li
      className={cn(
        'group flex flex-col gap-1 rounded-xl px-3 py-2 text-sm transition-colors hover:bg-[rgba(255,255,255,0.04)]',
        className,
      )}
      {...props}
    />
  )
}

export function QueueItemIndicator({
  completed = false,
  className,
  ...props
}: ComponentProps<'span'> & { completed?: boolean }) {
  return (
    <span
      className={cn(
        'mt-0.5 inline-block size-2.5 rounded-full border',
        completed
          ? 'border-muted-foreground/20 bg-muted-foreground/10'
          : 'border-muted-foreground/50',
        className,
      )}
      {...props}
    />
  )
}

export function QueueItemContent({
  completed = false,
  className,
  ...props
}: ComponentProps<'span'> & { completed?: boolean }) {
  return (
    <span
      className={cn(
        'line-clamp-1 grow break-words',
        completed ? 'text-muted-foreground/50 line-through' : 'text-muted-foreground',
        className,
      )}
      {...props}
    />
  )
}

export function QueueItemDescription({
  completed = false,
  className,
  ...props
}: ComponentProps<'div'> & { completed?: boolean }) {
  return (
    <div
      className={cn(
        'ml-6 text-xs',
        completed ? 'text-muted-foreground/40 line-through' : 'text-muted-foreground',
        className,
      )}
      {...props}
    />
  )
}

export function QueueItemActions({ className, ...props }: ComponentProps<'div'>) {
  return <div className={cn('flex gap-1', className)} {...props} />
}

export function QueueItemAction({
  className,
  ...props
}: Omit<ComponentProps<typeof Button>, 'variant' | 'size'>) {
  return (
    <Button
      className={cn(
        'size-auto rounded p-1 text-muted-foreground opacity-0 transition-opacity hover:bg-muted-foreground/10 hover:text-foreground group-hover:opacity-100',
        className,
      )}
      size="icon"
      type="button"
      variant="ghost"
      {...props}
    />
  )
}

export function QueueItemAttachment({ className, ...props }: ComponentProps<'div'>) {
  return <div className={cn('mt-1 flex flex-wrap gap-2', className)} {...props} />
}

export function QueueItemImage({ className, src, alt, ...props }: ComponentProps<'img'>) {
  if (!src) return null
  return (
    <Image
      src={src}
      alt={alt || ''}
      width={32}
      height={32}
      className={cn('h-8 w-8 rounded border object-cover', className)}
      {...(props as any)}
    />
  )
}

export function QueueItemFile({ children, className, ...props }: ComponentProps<'span'>) {
  return (
    <span
      className={cn(
        'flex items-center gap-1 rounded border border-[rgba(135,175,255,0.12)] bg-[rgba(255,255,255,0.04)] px-2 py-1 text-xs',
        className,
      )}
      {...props}
    >
      <PaperclipIcon size={12} />
      <span className="max-w-[100px] truncate">{children}</span>
    </span>
  )
}
