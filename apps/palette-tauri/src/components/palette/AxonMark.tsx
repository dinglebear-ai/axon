export function AxonMark({ size = 24 }: { size?: number }) {
  return (
    <svg className="axon-mark" width={size} height={size} viewBox="0 0 64 64" fill="none" aria-hidden="true">
      <g stroke="var(--aurora-border-strong)" strokeWidth="2" strokeLinecap="round">
        <path d="M22 9 Q28 14 31 17" />
        <path d="M32 7 L32 16" />
        <path d="M42 9 Q36 14 33 17" />
      </g>
      <line x1="32" y1="22" x2="32" y2="42" stroke="var(--aurora-border-strong)" strokeWidth="2" strokeDasharray="2.5 3.5" />
      <circle className="axon-node axon-node-1" cx="32" cy="20" r="5.2" fill="var(--aurora-border-strong)" stroke="var(--aurora-accent-strong)" strokeWidth="1.8" />
      <circle className="axon-node axon-node-2" cx="32" cy="30" r="5.2" fill="var(--aurora-accent-deep)" stroke="var(--aurora-accent-strong)" strokeWidth="1.8" />
      <circle className="axon-node axon-node-3" cx="32" cy="40" r="5.2" fill="var(--aurora-accent-primary)" stroke="var(--aurora-accent-strong)" strokeWidth="1.8" />
      <circle className="axon-node axon-node-4" cx="32" cy="50" r="5.2" fill="var(--aurora-accent-strong)" />
      <circle cx="32" cy="50" r="8" fill="none" stroke="var(--aurora-accent-strong)" strokeWidth="1.2" opacity="0.4" />
      <g stroke="var(--aurora-accent-strong)" strokeWidth="2" strokeLinecap="round">
        <path d="M28 53 Q23 58 19 62" />
        <path d="M32 54 L32 62" />
        <path d="M36 53 Q41 58 45 62" />
      </g>
    </svg>
  );
}
