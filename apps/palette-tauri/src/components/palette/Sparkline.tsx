import { memo } from "react";

interface SparklineProps {
  values: number[];
  /** Rendered width/height in px. */
  width?: number;
  height?: number;
  ariaLabel?: string;
}

// Tiny inline bar sparkline for a short numeric series (e.g. 7-day growth).
// Pure SVG, Aurora-tokened, no deps. Bars are normalized to the series max so a
// flat series renders as a baseline rather than empty.
export const Sparkline = memo(function Sparkline({
  values,
  width = 132,
  height = 28,
  ariaLabel,
}: SparklineProps) {
  const clean = values.filter((v) => Number.isFinite(v));
  if (clean.length === 0) return null;
  const max = Math.max(...clean, 1);
  const gap = 2;
  const barWidth = Math.max(1, (width - gap * (clean.length - 1)) / clean.length);
  const label = ariaLabel ?? `Sparkline of ${clean.length} values, latest ${clean[clean.length - 1]}`;

  return (
    <svg
      className="spark-svg"
      viewBox={`0 0 ${width} ${height}`}
      width={width}
      height={height}
      role="img"
      aria-label={label}
      preserveAspectRatio="none"
    >
      {clean.map((value, index) => {
        const barHeight = Math.max(1, (value / max) * (height - 2));
        const x = index * (barWidth + gap);
        const y = height - barHeight;
        const last = index === clean.length - 1;
        return (
          <rect
            key={index}
            x={x}
            y={y}
            width={barWidth}
            height={barHeight}
            rx={1}
            className={last ? "spark-bar spark-bar-last" : "spark-bar"}
          />
        );
      })}
    </svg>
  );
});
