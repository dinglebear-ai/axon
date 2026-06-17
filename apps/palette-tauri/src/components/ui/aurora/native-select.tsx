"use client"

import * as React from "react"
import { ChevronDown } from "lucide-react"

import { cn } from "@/lib/utils"

export interface NativeSelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> {
  placeholder?: string
}

const NativeSelect = React.forwardRef<HTMLSelectElement, NativeSelectProps>(
  ({ className, style, children, placeholder, disabled, ...props }, ref) => (
    <span className="relative inline-flex w-full items-center">
      <select
        ref={ref}
        disabled={disabled}
        className={cn(
          "h-9 w-full appearance-none rounded-[var(--aurora-radius-1)] border px-3 py-2 pr-9",
          "font-[var(--aurora-font-sans)]",
          "text-[var(--aurora-text-primary)]",
          "transition-all duration-150 ease-out",
          "focus-visible:outline-none",
          "cursor-pointer",
          "disabled:pointer-events-none disabled:opacity-45 disabled:cursor-not-allowed",
          className,
        )}
        style={{
          background: "var(--aurora-control-surface)",
          borderColor: "var(--aurora-border-strong)",
          color: "var(--aurora-text-primary)",
          fontSize: "var(--aurora-type-body-sm)",
          fontWeight: "var(--aurora-weight-body)",
          letterSpacing: "var(--aurora-letter-ui)",
          lineHeight: "var(--aurora-line-ui)",
          ...style,
        }}
        onFocus={(event) => {
          event.currentTarget.dataset.previousBorderColor = event.currentTarget.style.borderColor
          event.currentTarget.dataset.previousBoxShadow = event.currentTarget.style.boxShadow
          event.currentTarget.style.borderColor = "color-mix(in srgb, var(--aurora-accent-primary) 42%, var(--aurora-border-strong))"
          event.currentTarget.style.boxShadow = "0 0 0 2px color-mix(in srgb, var(--aurora-accent-primary) 22%, transparent)"
          props.onFocus?.(event)
        }}
        onBlur={(event) => {
          event.currentTarget.style.borderColor = event.currentTarget.dataset.previousBorderColor ?? ""
          event.currentTarget.style.boxShadow = event.currentTarget.dataset.previousBoxShadow ?? ""
          props.onBlur?.(event)
        }}
        {...props}
      >
        {placeholder ? (
          <option value="" disabled>
            {placeholder}
          </option>
        ) : null}
        {children}
      </select>
      <ChevronDown
        aria-hidden="true"
        className="pointer-events-none absolute right-3 size-4 text-[var(--aurora-text-muted)]"
        strokeWidth={1.75}
      />
    </span>
  ),
)
NativeSelect.displayName = "NativeSelect"

export { NativeSelect }
export default NativeSelect
