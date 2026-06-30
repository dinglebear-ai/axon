import * as React from "react";
import { ExternalLink, Globe } from "lucide-react";

export interface SourceItem {
  title: string;
  href?: string;
  description?: string;
  badge?: string;
}

export interface SourceProps extends React.HTMLAttributes<HTMLElement> {
  source: SourceItem;
  index?: number;
}

function hostname(href?: string): string | null {
  if (!href) return null;
  try {
    return new URL(href).hostname.replace(/^www\./, "");
  } catch {
    return (
      href
        .replace(/^https?:\/\//, "")
        .replace(/^www\./, "")
        .split("/")[0] || null
    );
  }
}

const Source = React.forwardRef<HTMLElement, SourceProps>(
  ({ className, source, index, ...props }, ref) => {
    const host = hostname(source.href);
    const classNames = ["aurora-source-card", className].filter(Boolean).join(" ");
    const content = (
      <>
        {index != null ? <span className="aurora-source-index">{index}</span> : null}
        <span className="aurora-source-body">
          <span className="aurora-source-title-row">
            <strong>{source.title}</strong>
            {source.badge ? <em>{source.badge}</em> : null}
          </span>
          {host ? (
            <span className="aurora-source-host">
              <Globe size={14} strokeWidth={1.7} aria-hidden="true" />
              <span>{host}</span>
            </span>
          ) : null}
          {source.description ? <small>{source.description}</small> : null}
        </span>
        {source.href ? <ExternalLink size={17} strokeWidth={1.7} aria-hidden="true" /> : null}
      </>
    );
    if (!source.href) {
      return (
        <div
          ref={ref as React.ForwardedRef<HTMLDivElement>}
          className={classNames}
          aria-disabled="true"
          {...props}
        >
          {content}
        </div>
      );
    }
    return (
      <a
        {...props}
        ref={ref as React.ForwardedRef<HTMLAnchorElement>}
        href={source.href}
        target="_blank"
        rel="noopener noreferrer"
        className={classNames}
      >
        {content}
      </a>
    );
  },
);
Source.displayName = "Source";

export { Source };
export default Source;
