import type * as React from "react"
import { Slot } from "@radix-ui/react-slot"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "@/lib/utils"
import { Spinner } from "@/components/ui/spinner"

const buttonVariants = cva(
  [
    "inline-flex items-center justify-center gap-2 whitespace-nowrap",
    "transition-all duration-150 ease-out",
    "disabled:pointer-events-none disabled:opacity-45",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--aurora-accent-primary)] focus-visible:ring-offset-0",
    "select-none cursor-pointer",
    "active:scale-[0.97]",
  ].join(" "),
  {
    variants: {
      variant: {
        aurora: [
          "border text-[var(--aurora-text-primary)]",
          "bg-transparent",
        ].join(" "),
        neutral: [
          "border text-[var(--aurora-text-primary)]",
          "bg-transparent",
        ].join(" "),
        rose: [
          "border text-[var(--aurora-text-primary)]",
          "bg-transparent",
        ].join(" "),
        violet: [
          "border text-[var(--aurora-text-primary)]",
          "bg-transparent",
        ].join(" "),
        ghost: [
          "border-transparent text-[var(--aurora-text-muted)]",
          "bg-transparent hover:text-[var(--aurora-text-primary)]",
        ].join(" "),
        destructive: [
          "border text-[var(--aurora-error)]",
          "bg-transparent",
        ].join(" "),
        plain: "border-transparent bg-transparent text-inherit",
      },
      size: {
        sm: "h-7 px-3 rounded-[7px]",
        default: "h-8 px-3.5 rounded-[8px]",
        lg: "h-10 px-5 rounded-[10px]",
        icon: "size-8 rounded-[8px] p-0",
        unstyled: "",
      },
    },
    defaultVariants: {
      variant: "aurora",
      size: "default",
    },
  }
)

type ButtonVariant = "aurora" | "neutral" | "rose" | "violet" | "ghost" | "destructive" | "plain"

// Variants whose resting/hover/active skin is fully driven by the
// `.aurora-btn[data-variant="…"]` CSS rules in aurora.css. Keeping the skin in
// CSS (instead of an inline-style table) lets tailwind-merge and selector
// specificity reason about overrides — and lets consumers win without
// !important (TW-M1 / M7). "plain" intentionally carries no skin.
const SKINNED_VARIANTS = new Set<ButtonVariant>([
  "aurora",
  "neutral",
  "rose",
  "violet",
  "ghost",
  "destructive",
])

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean
  loading?: boolean
  ref?: React.Ref<HTMLButtonElement>
}

function Button({
  className,
  variant,
  size,
  asChild = false,
  loading = false,
  style,
  children,
  disabled,
  onClick,
  ref,
  ...props
}: ButtonProps) {
  {
    const Comp = asChild ? Slot : "button"

    const resolvedVariant: ButtonVariant = (variant as ButtonVariant) ?? "aurora"
    const skinned = SKINNED_VARIANTS.has(resolvedVariant)

    const typographyStyle =
      resolvedVariant === "plain" && size === "unstyled"
        ? {}
        : {
            fontFamily: "var(--aurora-font-sans)",
            fontSize: size === "lg" ? "14px" : size === "sm" ? "12px" : "13px",
            fontWeight: size === "lg" ? 680 : 650,
            letterSpacing: "0.012em",
            lineHeight: "var(--aurora-line-ui)",
          }

    // Map button size to a spinner size that fits inside it
    const spinnerSize: "sm" | "default" =
      size === "sm" ? "sm" : "default"

    // Map variant to an appropriate spinner tone
    const spinnerTone: "cyan" | "rose" | "muted" =
      resolvedVariant === "rose"
        ? "rose"
        : resolvedVariant === "destructive" || resolvedVariant === "ghost" || resolvedVariant === "plain"
        ? "muted"
        : "cyan"

    const isDisabled = disabled || loading
    const disabledChildProps = asChild && isDisabled
      ? {
          "aria-disabled": true,
          tabIndex: -1,
        }
      : {}

    return (
      <Comp
        ref={ref}
        aria-busy={loading ? "true" : undefined}
        disabled={asChild ? undefined : isDisabled}
        data-variant={skinned ? resolvedVariant : undefined}
        className={cn(
          skinned && "aurora-btn",
          buttonVariants({ variant, size }),
          asChild && isDisabled && "pointer-events-none opacity-45",
          className
        )}
        style={{
          ...typographyStyle,
          // Preserve width during loading so layout doesn't shift
          ...(loading ? { minWidth: "var(--btn-loading-width, auto)" } : {}),
          ...style,
        }}
        {...props}
        onClick={(event: React.MouseEvent<HTMLButtonElement>) => {
          if (isDisabled) {
            event.preventDefault()
            event.stopPropagation()
            return
          }
          onClick?.(event)
        }}
        {...disabledChildProps}
      >
        {loading ? (
          <Spinner size={spinnerSize} tone={spinnerTone} aria-hidden="true" />
        ) : (
          children
        )}
      </Comp>
    )
  }
}
Button.displayName = "Button"

export { Button, buttonVariants }
export type { ButtonVariant }
export default Button
