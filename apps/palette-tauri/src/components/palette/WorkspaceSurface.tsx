import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

export function WorkspaceSurface({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return <div className={`workspace-surface aurora-scrollbar ${className}`}>{children}</div>;
}

export function WorkspaceHeader({
  icon: Icon,
  eyebrow,
  title,
  description,
  metrics = [],
  actions,
}: {
  icon: LucideIcon;
  eyebrow: string;
  title: string;
  description?: string;
  metrics?: Array<{ label: string; value: ReactNode }>;
  actions?: ReactNode;
}) {
  return (
    <header className="workspace-header">
      <span className="workspace-header-icon" aria-hidden="true">
        <Icon size={17} strokeWidth={1.65} />
      </span>
      <div className="workspace-header-copy">
        <span className="workspace-eyebrow">{eyebrow}</span>
        <h3>{title}</h3>
        {description ? <p>{description}</p> : null}
      </div>
      {metrics.length ? (
        <div className="workspace-metrics">
          {metrics.map((metric) => (
            <span key={metric.label}>
              <strong>{metric.value}</strong>
              <small>{metric.label}</small>
            </span>
          ))}
        </div>
      ) : null}
      {actions ? <div className="workspace-header-actions">{actions}</div> : null}
    </header>
  );
}

export function WorkspaceEmpty({
  icon: Icon,
  title,
  description,
  action,
}: {
  icon: LucideIcon;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="workspace-empty" role="status">
      <span aria-hidden="true">
        <Icon size={26} strokeWidth={1.5} />
      </span>
      <strong>{title}</strong>
      <p>{description}</p>
      {action}
    </div>
  );
}
