"use client"

import * as React from "react"
import { cn } from "@/lib/utils"

export type InputState = "error" | "warn" | "success"
export type InputSize = "sm" | "default" | "lg"

export interface InputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "size"> {
  /** Optional leading icon or adornment */
  startAdornment?: React.ReactNode
  /** Optional trailing icon or adornment */
  endAdornment?: React.ReactNode
  /**
   * Validation state. Sets border color and glow ring using Aurora status tokens.
   * - error: --aurora-error border + ring
   * - warn:  --aurora-warn border + ring
   * - success: --aurora-success border + ring
   */
  state?: InputState
  /**
   * Convenience alias for state="error". When both are set, `state` takes precedence.
   */
  error?: boolean
  /**
   * Input size variant.
   * - sm: h-7, caption font, px-2.5
   * - default: h-9 (original), body-sm font, px-3
   * - lg: h-10, control font, px-3.5
   * @default "default"
   */
  size?: InputSize
  /**
   * When true and the input has a value, shows a clear (×) button as the end adornment.
   * The clear button calls `onClear` if provided, otherwise clears the rendered input.
   */
  clearable?: boolean
  /** Callback fired when the clear button is clicked. Escape hatch for controlled inputs. */
  onClear?: () => void
}

/** Token map for validation states */
const STATE_TOKENS = {
  error: {
    border: "var(--aurora-error)",
    ring: "var(--aurora-error)",
  },
  warn: {
    border: "var(--aurora-warn)",
    ring: "var(--aurora-warn)",
  },
  success: {
    border: "var(--aurora-success)",
    ring: "var(--aurora-success)",
  },
} as const

/** Resting box-shadow for a state — subtle 1px ring */
function stateRestShadow(color: string): string {
  return `0 0 0 1px color-mix(in srgb, ${color} 30%, transparent)`
}

/** Focused box-shadow for a state — intensified double ring */
function stateFocusShadow(color: string): string {
  return [
    `0 0 0 3px color-mix(in srgb, ${color} 22%, transparent)`,
    `0 0 0 1px color-mix(in srgb, ${color} 55%, transparent)`,
  ].join(", ")
}

/** Default focus box-shadow (no state) */
const DEFAULT_FOCUS_SHADOW = [
  "0 0 0 3px color-mix(in srgb, var(--aurora-accent-primary) 22%, transparent)",
  "0 0 0 1px color-mix(in srgb, var(--aurora-accent-primary) 45%, transparent)",
].join(", ")

const sizeClasses: Record<InputSize, string> = {
  sm: "h-7 px-2.5",
  default: "h-9 px-3",
  lg: "h-10 px-3.5",
}

const sizeFontTokens: Record<InputSize, string> = {
  sm: "var(--aurora-type-caption)",
  default: "var(--aurora-type-body-sm)",
  lg: "var(--aurora-type-control)",
}

const Input = React.forwardRef<HTMLInputElement, InputProps>(
  (
    {
      className,
      type = "text",
      startAdornment,
      endAdornment,
      style,
      state: stateProp,
      error,
      size = "default",
      clearable,
      onClear,
      value,
      defaultValue,
      onChange,
      ...props
    },
    ref
  ) => {
    const inputRef = React.useRef<HTMLInputElement>(null)
    React.useImperativeHandle(ref, () => inputRef.current as HTMLInputElement)

    // Resolve effective state — explicit `state` wins over `error` shorthand
    const effectiveState: InputState | undefined = stateProp ?? (error ? "error" : undefined)
    const tokens = effectiveState ? STATE_TOKENS[effectiveState] : null

    // Track internal value for clearable visibility when uncontrolled
    const [internalValue, setInternalValue] = React.useState<string>(
      typeof defaultValue === "string" || typeof defaultValue === "number"
        ? String(defaultValue)
        : ""
    )

    // Determine whether the input currently has a value
    const isControlled = value !== undefined
    const currentValue = isControlled ? String(value ?? "") : internalValue
    const showClearButton = clearable && currentValue.length > 0

    // Build the effective end adornment — clear button takes precedence when visible
    const effectiveEndAdornment = showClearButton ? (
      <button
        type="button"
        aria-label="Clear"
        className={cn(
          "pointer-events-auto",
          "flex h-4 w-4 items-center justify-center rounded-full",
          "text-[var(--aurora-text-muted)] hover:text-[var(--aurora-text-primary)]",
          "hover:bg-[var(--aurora-hover-bg)]",
          "transition-colors duration-100",
          "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[var(--aurora-accent-primary)]",
          "text-[10px] leading-none select-none"
        )}
        onMouseDown={(e) => {
          // Prevent input blur before we fire onChange
          e.preventDefault()
        }}
        onClick={() => {
          if (onClear) {
            onClear()
          } else {
            const input = inputRef.current
            if (!input) return
            const nativeInputValueSetter = Object.getOwnPropertyDescriptor(
              window.HTMLInputElement.prototype,
              "value"
            )?.set
            nativeInputValueSetter?.call(input, "")
            if (onChange) {
              input.dispatchEvent(new Event("input", { bubbles: true }))
            }
          }
          // Always update internal state for uncontrolled
          if (!isControlled) {
            setInternalValue("")
          }
        }}
      >
        ×
      </button>
    ) : endAdornment

    const hasStart = Boolean(startAdornment)
    const hasEnd = Boolean(effectiveEndAdornment)

    // Compute adornment padding — larger sizes need wider offsets
    const startPadClass =
      hasStart
        ? size === "sm"
          ? "pl-8"
          : size === "lg"
          ? "pl-10"
          : "pl-9"
        : undefined
    const endPadClass =
      hasEnd
        ? size === "sm"
          ? "pr-8"
          : size === "lg"
          ? "pr-10"
          : "pr-9"
        : undefined

    return (
      <div className="relative inline-flex w-full items-center">
        {hasStart && (
          <span
            className="pointer-events-none absolute left-3 z-10 flex items-center text-[var(--aurora-text-muted)]"
            aria-hidden="true"
          >
            {startAdornment}
          </span>
        )}

        <input
          ref={inputRef}
          type={type}
          value={value}
          defaultValue={defaultValue}
          className={cn(
            // Layout — size-driven
            "w-full py-2",
            sizeClasses[size],
            // Typography
            "font-[var(--aurora-font-sans)]",
            "text-[var(--aurora-text-primary)]",
            "placeholder:text-[var(--aurora-text-muted)]",
            // Background & border
            "border",
            tokens
              ? `border-[${tokens.border}]`
              : "border-[var(--aurora-border-strong)]",
            // Rounded
            "rounded-[var(--aurora-radius-1)]",
            // Transitions
            "transition-all duration-150 ease-out",
            // Focus ring — handled inline via onFocus/onBlur
            "focus-visible:outline-none",
            // Disabled
            "disabled:pointer-events-none disabled:opacity-45 disabled:cursor-not-allowed",
            // Adornment padding adjustments
            startPadClass,
            endPadClass,
            className
          )}
          style={{
            background: "var(--aurora-control-surface)",
            fontSize: sizeFontTokens[size],
            fontWeight: "var(--aurora-weight-body)",
            letterSpacing: "var(--aurora-letter-ui)",
            lineHeight: "var(--aurora-line-ui)",
            // State border color — inline so it wins over Tailwind
            ...(tokens
              ? { borderColor: tokens.border }
              : {}),
            // Resting glow ring for validation state
            ...(tokens
              ? { boxShadow: stateRestShadow(tokens.ring) }
              : {}),
            ...style,
          }}
          onChange={(e) => {
            if (!isControlled) {
              setInternalValue(e.target.value)
            }
            onChange?.(e)
          }}
          onFocus={(e) => {
            if (tokens) {
              e.currentTarget.style.borderColor = tokens.border
              e.currentTarget.style.boxShadow = stateFocusShadow(tokens.ring)
            } else {
              e.currentTarget.style.borderColor = "var(--aurora-border-strong)"
              e.currentTarget.style.boxShadow = DEFAULT_FOCUS_SHADOW
            }
            props.onFocus?.(e)
          }}
          onBlur={(e) => {
            if (tokens) {
              // Restore resting state ring on blur
              e.currentTarget.style.borderColor = tokens.border
              e.currentTarget.style.boxShadow = stateRestShadow(tokens.ring)
            } else {
              e.currentTarget.style.boxShadow = "none"
            }
            props.onBlur?.(e)
          }}
          {...props}
        />

        {hasEnd && (
          <span
            className="pointer-events-auto absolute right-3 z-10 flex items-center text-[var(--aurora-text-muted)]"
          >
            {effectiveEndAdornment}
          </span>
        )}
      </div>
    )
  }
)
Input.displayName = "Input"

export { Input }
export default Input
