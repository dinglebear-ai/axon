import type * as React from "react"

export interface ScrollAreaProps extends React.HTMLAttributes<HTMLDivElement> {
  viewportClassName?: string
  ref?: React.Ref<HTMLDivElement>
}

export function ScrollArea({
  className,
  viewportClassName,
  style,
  children,
  ref,
  ...props
}: ScrollAreaProps) {
  return (
    <div
      ref={ref}
      className={["overflow-hidden rounded-[8px] border", className].filter(Boolean).join(" ")}
      style={{
        background: "var(--aurora-panel-medium)",
        borderColor: "var(--aurora-border-default)",
        ...style,
      }}
      {...props}
    >
      <div className={["max-h-72 overflow-auto aurora-scrollbar", viewportClassName].filter(Boolean).join(" ")}>
        {children}
      </div>
    </div>
  )
}
ScrollArea.displayName = "ScrollArea"

export default ScrollArea

