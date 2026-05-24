import * as React from "react"
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

// Single source of truth for all per-variant styling
interface VariantConfig {
  style: React.CSSProperties
  hoverClass: string
  activeClass: string
  typographyExtra?: React.CSSProperties
}

const VARIANT_CONFIG: Record<ButtonVariant, VariantConfig> = {
  aurora: {
    style: {
      // Shadow set via CSS custom property so transition-all can interpolate it
      borderColor: "color-mix(in srgb, var(--aurora-accent-primary) 42%, var(--aurora-border-strong))",
      background: [
        "linear-gradient(180deg, color-mix(in srgb, var(--aurora-accent-primary) 10%, transparent), transparent 58%)",
        "var(--aurora-control-surface)",
      ].join(", "),
      boxShadow: [
        "inset 0 1px 0 rgba(255,255,255,0.055)",
        "0 0 0 1px color-mix(in srgb, var(--aurora-accent-primary) 16%, transparent)",
        "0 0 10px color-mix(in srgb, var(--aurora-accent-primary) 12%, transparent)",
      ].join(", "),
    },
    hoverClass:
      "hover:border-[color-mix(in_srgb,var(--aurora-accent-primary)_58%,var(--aurora-border-strong))] hover:bg-[color-mix(in_srgb,var(--aurora-accent-primary)_8%,var(--aurora-control-surface))]",
    activeClass:
      "active:bg-[color-mix(in_srgb,var(--aurora-accent-primary)_14%,var(--aurora-control-surface))]",
  },
  neutral: {
    style: {
      borderColor: "var(--aurora-border-strong)",
      background: "var(--aurora-control-surface)",
      boxShadow: "inset 0 1px 0 rgba(255,255,255,0.045)",
    },
    hoverClass:
      "hover:border-[var(--aurora-border-strong)] hover:bg-[var(--aurora-hover-bg)]",
    activeClass:
      "active:bg-[color-mix(in_srgb,var(--aurora-text-primary)_8%,var(--aurora-hover-bg))]",
  },
  rose: {
    style: {
      borderColor:
        "color-mix(in srgb, var(--aurora-accent-pink) 52%, var(--aurora-border-strong))",
      background: [
        "linear-gradient(180deg, color-mix(in srgb, var(--aurora-accent-pink) 14%, transparent), transparent 58%)",
        "var(--aurora-control-surface)",
      ].join(", "),
      boxShadow: [
        "inset 0 1px 0 rgba(255,255,255,0.06)",
        "0 0 0 1px color-mix(in srgb, var(--aurora-accent-pink) 18%, transparent)",
        "0 0 13px color-mix(in srgb, var(--aurora-accent-pink) 16%, transparent)",
      ].join(", "),
    },
    hoverClass:
      "hover:border-[color-mix(in_srgb,var(--aurora-accent-pink)_68%,var(--aurora-border-strong))] hover:bg-[color-mix(in_srgb,var(--aurora-accent-pink)_10%,var(--aurora-control-surface))] hover:[box-shadow:inset_0_1px_0_rgba(255,255,255,0.08),0_0_0_1px_color-mix(in_srgb,var(--aurora-accent-pink)_24%,transparent),0_0_18px_color-mix(in_srgb,var(--aurora-accent-pink)_24%,transparent)]",
    activeClass:
      "active:bg-[color-mix(in_srgb,var(--aurora-accent-pink)_18%,var(--aurora-control-surface))]",
  },
  violet: {
    style: {
      borderColor:
        "color-mix(in srgb, var(--aurora-accent-violet) 42%, var(--aurora-border-strong))",
      background: [
        "linear-gradient(180deg, color-mix(in srgb, var(--aurora-accent-violet) 10%, transparent), transparent 58%)",
        "var(--aurora-control-surface)",
      ].join(", "),
      boxShadow: [
        "inset 0 1px 0 rgba(255,255,255,0.055)",
        "0 0 0 1px color-mix(in srgb, var(--aurora-accent-violet) 16%, transparent)",
        "0 0 10px color-mix(in srgb, var(--aurora-accent-violet) 12%, transparent)",
      ].join(", "),
    },
    hoverClass:
      "hover:border-[color-mix(in_srgb,var(--aurora-accent-violet)_58%,var(--aurora-border-strong))] hover:bg-[color-mix(in_srgb,var(--aurora-accent-violet)_10%,var(--aurora-control-surface))] hover:[box-shadow:inset_0_1px_0_rgba(255,255,255,0.06),0_0_0_1px_color-mix(in_srgb,var(--aurora-accent-violet)_24%,transparent),0_0_15px_color-mix(in_srgb,var(--aurora-accent-violet)_20%,transparent)]",
    activeClass:
      "active:bg-[color-mix(in_srgb,var(--aurora-accent-violet)_16%,var(--aurora-control-surface))]",
  },
  ghost: {
    style: {},
    hoverClass: "hover:bg-[var(--aurora-hover-bg)]",
    activeClass: "",
  },
  destructive: {
    style: {
      borderColor:
        "color-mix(in srgb, var(--aurora-error) 42%, var(--aurora-border-strong))",
      background: [
        "linear-gradient(180deg, color-mix(in srgb, var(--aurora-error) 9%, transparent), transparent 58%)",
        "var(--aurora-control-surface)",
      ].join(", "),
      boxShadow: [
        "inset 0 1px 0 rgba(255,255,255,0.045)",
        "0 0 0 1px color-mix(in srgb, var(--aurora-error) 14%, transparent)",
      ].join(", "),
    },
    hoverClass:
      "hover:border-[color-mix(in_srgb,var(--aurora-error)_58%,var(--aurora-border-strong))] hover:bg-[color-mix(in_srgb,var(--aurora-error)_7%,var(--aurora-control-surface))]",
    activeClass: "",
  },
  plain: {
    style: {},
    hoverClass: "",
    activeClass: "",
  },
}

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean
  loading?: boolean
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      className,
      variant,
      size,
      asChild = false,
      loading = false,
      style,
      children,
      disabled,
      onClick,
      ...props
    },
    ref
  ) => {
    const Comp = asChild ? Slot : "button"

    const resolvedVariant: ButtonVariant = (variant as ButtonVariant) ?? "aurora"
    const config = VARIANT_CONFIG[resolvedVariant] ?? VARIANT_CONFIG.aurora

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
        className={cn(
          buttonVariants({ variant, size }),
          config.hoverClass,
          config.activeClass,
          asChild && isDisabled && "pointer-events-none opacity-45",
          className
        )}
        style={{
          ...typographyStyle,
          ...config.style,
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
)
Button.displayName = "Button"

export { Button, buttonVariants }
export type { ButtonVariant }
export default Button
