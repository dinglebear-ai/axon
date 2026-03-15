'use client'

import type { FileUIPart, UIMessage } from 'ai'
import { ChevronLeftIcon, ChevronRightIcon, PaperclipIcon, XIcon } from 'lucide-react'
import Image from 'next/image'
import type { ComponentProps, HTMLAttributes, ReactElement } from 'react'
import { createContext, memo, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import { Streamdown } from 'streamdown'
import { Button } from '@/components/ui/button'
import { ButtonGroup, ButtonGroupText } from '@/components/ui/button-group'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'

export function Message({
  className,
  from,
  ...props
}: HTMLAttributes<HTMLDivElement> & { from: UIMessage['role'] }) {
  return (
    <div
      className={cn(
        'group flex w-full max-w-[95%] flex-col gap-2',
        from === 'user' ? 'is-user ml-auto justify-end' : 'is-assistant',
        className,
      )}
      {...props}
    />
  )
}

export function MessageContent({ children, className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        'flex w-fit max-w-full min-w-0 flex-col gap-2 overflow-hidden text-sm',
        'group-[.is-user]:ml-auto group-[.is-user]:rounded-xl group-[.is-user]:bg-[rgba(255,135,175,0.14)] group-[.is-user]:px-3.5 group-[.is-user]:py-2.5 lg:group-[.is-user]:px-3 lg:group-[.is-user]:py-2',
        'group-[.is-assistant]:text-foreground',
        className,
      )}
      {...props}
    >
      {children}
    </div>
  )
}

export function MessageActions({ className, children, ...props }: ComponentProps<'div'>) {
  return (
    <div className={cn('flex items-center gap-1', className)} {...props}>
      {children}
    </div>
  )
}

export function MessageAction({
  tooltip,
  children,
  label,
  variant = 'ghost',
  size = 'icon-sm',
  ...props
}: ComponentProps<typeof Button> & { tooltip?: string; label?: string }) {
  const button = (
    <Button size={size} type="button" variant={variant} {...props}>
      {children}
      <span className="sr-only">{label || tooltip}</span>
    </Button>
  )

  if (!tooltip) return button

  return (
    <Tooltip>
      <TooltipTrigger asChild>{button}</TooltipTrigger>
      <TooltipContent>
        <p>{tooltip}</p>
      </TooltipContent>
    </Tooltip>
  )
}

type MessageBranchContextValue = {
  currentBranch: number
  totalBranches: number
  goToNext: () => void
  goToPrevious: () => void
  branches: ReactElement[]
  setBranches: (branches: ReactElement[]) => void
}

const MessageBranchContext = createContext<MessageBranchContextValue | null>(null)

function useMessageBranch() {
  const context = useContext(MessageBranchContext)
  if (!context) throw new Error('MessageBranch components must be used within MessageBranch')
  return context
}

export function MessageBranch({
  defaultBranch = 0,
  onBranchChange,
  className,
  ...props
}: HTMLAttributes<HTMLDivElement> & {
  defaultBranch?: number
  onBranchChange?: (branchIndex: number) => void
}) {
  const [currentBranch, setCurrentBranch] = useState(defaultBranch)
  const [branches, setBranches] = useState<ReactElement[]>([])

  const handleBranchChange = useCallback(
    (nextBranch: number) => {
      setCurrentBranch(nextBranch)
      onBranchChange?.(nextBranch)
    },
    [onBranchChange],
  )

  const goToPrevious = useCallback(() => {
    handleBranchChange(currentBranch > 0 ? currentBranch - 1 : branches.length - 1)
  }, [handleBranchChange, currentBranch, branches.length])

  const goToNext = useCallback(() => {
    handleBranchChange(currentBranch < branches.length - 1 ? currentBranch + 1 : 0)
  }, [handleBranchChange, currentBranch, branches.length])

  const contextValue = useMemo(
    () => ({
      currentBranch,
      totalBranches: branches.length,
      goToNext,
      goToPrevious,
      branches,
      setBranches,
    }),
    [currentBranch, branches, goToNext, goToPrevious, setBranches],
  )

  return (
    <MessageBranchContext.Provider value={contextValue}>
      <div className={cn('grid w-full gap-2 [&>div]:pb-0', className)} {...props} />
    </MessageBranchContext.Provider>
  )
}

export function MessageBranchContent({ children, ...props }: HTMLAttributes<HTMLDivElement>) {
  const { currentBranch, setBranches } = useMessageBranch()
  const childrenArray = useMemo(() => (Array.isArray(children) ? children : [children]), [children])

  useEffect(() => {
    setBranches(childrenArray as ReactElement[])
  }, [childrenArray.length, setBranches])

  return childrenArray.map((branch, index) => (
    <div
      key={(branch as ReactElement).key ?? index}
      className={cn(
        'grid gap-2 overflow-hidden [&>div]:pb-0',
        index === currentBranch ? 'block' : 'hidden',
      )}
      {...props}
    >
      {branch}
    </div>
  ))
}

export function MessageBranchSelector({ ...props }: ComponentProps<typeof ButtonGroup>) {
  const { totalBranches } = useMessageBranch()
  if (totalBranches <= 1) return null
  return <ButtonGroup orientation="horizontal" {...props} />
}

export function MessageBranchPrevious({ children, ...props }: ComponentProps<typeof Button>) {
  const { goToPrevious, totalBranches } = useMessageBranch()
  return (
    <Button
      aria-label="Previous branch"
      disabled={totalBranches <= 1}
      onClick={goToPrevious}
      size="icon-sm"
      type="button"
      variant="ghost"
      {...props}
    >
      {children ?? <ChevronLeftIcon size={14} />}
    </Button>
  )
}

export function MessageBranchNext({ children, ...props }: ComponentProps<typeof Button>) {
  const { goToNext, totalBranches } = useMessageBranch()
  return (
    <Button
      aria-label="Next branch"
      disabled={totalBranches <= 1}
      onClick={goToNext}
      size="icon-sm"
      type="button"
      variant="ghost"
      {...props}
    >
      {children ?? <ChevronRightIcon size={14} />}
    </Button>
  )
}

export function MessageBranchPage({ className, ...props }: ComponentProps<'span'>) {
  const { currentBranch, totalBranches } = useMessageBranch()
  return (
    <ButtonGroupText
      className={cn('border-none bg-transparent text-muted-foreground shadow-none', className)}
      {...props}
    >
      {currentBranch + 1} of {totalBranches}
    </ButtonGroupText>
  )
}

export const MessageResponse = memo(
  function MessageResponse(props: ComponentProps<typeof Streamdown>) {
    return (
      <Streamdown
        className={cn(
          'size-full min-w-0 break-words text-[10px] leading-[1.4] [&>*:first-child]:mt-0 [&>*:last-child]:mb-0 [&_p]:my-1 [&_p]:text-[10px] [&_p]:leading-[1.4] [&_h1]:mt-1.5 [&_h1]:mb-0.5 [&_h1]:text-[12px] [&_h1]:font-semibold [&_h1]:leading-[1.25] [&_h2]:mt-1.5 [&_h2]:mb-0.5 [&_h2]:text-[11px] [&_h2]:font-semibold [&_h2]:leading-[1.25] [&_h3]:mt-1 [&_h3]:mb-0.5 [&_h3]:text-[10px] [&_h3]:font-semibold [&_h3]:leading-[1.25] [&_ol]:my-1 [&_ol]:list-inside [&_ol]:list-decimal [&_ol]:pl-2 [&_ul]:my-1 [&_ul]:list-inside [&_ul]:list-disc [&_ul]:pl-2 [&_li]:ml-0 [&_li]:text-[10px] [&_li]:leading-[1.4] [&_li+li]:mt-0.5 [&_ol_ol]:pl-3 [&_ul_ul]:list-[circle] [&_ul_ul]:pl-3 [&_pre]:my-1 [&_pre]:text-[9px] [&_code]:text-[9px]',
          props.className,
        )}
        {...props}
      />
    )
  },
  (prevProps, nextProps) => prevProps.children === nextProps.children,
)

export function MessageAttachment({
  data,
  className,
  onRemove,
  ...props
}: HTMLAttributes<HTMLDivElement> & { data: FileUIPart; onRemove?: () => void }) {
  const filename = data.filename || ''
  const isImage = Boolean(data.mediaType?.startsWith('image/') && data.url)
  const attachmentLabel = filename || (isImage ? 'Image' : 'Attachment')

  return (
    <div className={cn('group relative size-24 overflow-hidden rounded-lg', className)} {...props}>
      {isImage ? (
        <>
          <Image
            alt={filename || 'attachment'}
            className="size-full object-cover"
            src={data.url}
            width={96}
            height={96}
          />
          {onRemove ? (
            <Button
              aria-label="Remove attachment"
              className="absolute right-2 top-2 size-6 rounded-full bg-background/80 p-0 opacity-0 backdrop-blur-sm transition-opacity group-hover:opacity-100 [&>svg]:size-3"
              onClick={(event) => {
                event.stopPropagation()
                onRemove()
              }}
              type="button"
              variant="ghost"
            >
              <XIcon />
            </Button>
          ) : null}
        </>
      ) : (
        <Tooltip>
          <TooltipTrigger asChild>
            <div className="flex size-full items-center justify-center rounded-lg bg-muted text-muted-foreground">
              <PaperclipIcon className="size-4" />
            </div>
          </TooltipTrigger>
          <TooltipContent>
            <p>{attachmentLabel}</p>
          </TooltipContent>
        </Tooltip>
      )}
    </div>
  )
}

export function MessageAttachments({ children, className, ...props }: ComponentProps<'div'>) {
  if (!children) return null
  return (
    <div className={cn('ml-auto flex w-fit flex-wrap items-start gap-2', className)} {...props}>
      {children}
    </div>
  )
}

export function MessageToolbar({ className, children, ...props }: ComponentProps<'div'>) {
  return (
    <div
      className={cn('mt-3 flex w-full items-center justify-between gap-3', className)}
      {...props}
    >
      {children}
    </div>
  )
}
