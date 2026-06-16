import * as React from "react"
import { cn } from "@/lib/utils"

export interface KbdProps extends React.HTMLAttributes<HTMLElement> {
  /**
   * Escape hatch. When true, renders a bare `<kbd>` with only `className` and the
   * forwarded props/ref — no inline style skin — so the consumer's CSS owns the
   * appearance. The styled path (default) is unaffected.
   * @default false
   */
  unstyled?: boolean
}

const Kbd = React.forwardRef<HTMLElement, KbdProps>(
  ({ className, style, unstyled = false, ...props }, ref) => {
    if (unstyled) {
      return <kbd ref={ref} className={className} {...props} />
    }

    return (
      <kbd
        ref={ref}
        className={cn("inline-flex min-w-5 items-center justify-center rounded-[5px] border px-1.5", className)}
        style={{
          background: "var(--aurora-control-surface)",
          borderColor: "var(--aurora-border-strong)",
          boxShadow: "inset 0 -1px 0 rgba(0,0,0,0.35), var(--aurora-highlight-medium)",
          color: "var(--aurora-text-muted)",
          fontFamily: "var(--aurora-font-mono)",
          fontSize: 11,
          fontWeight: 600,
          height: 20,
          lineHeight: 1,
          ...style,
        }}
        {...props}
      />
    )
  }
)
Kbd.displayName = "Kbd"

export { Kbd }
export default Kbd
