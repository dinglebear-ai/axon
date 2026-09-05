export function startCompletionDrivenPolling({
  poll,
  intervalMs,
  isVisible
}: {
  poll: () => Promise<void>;
  intervalMs: number;
  isVisible: () => boolean;
}): () => void {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const schedule = () => {
    if (!stopped) timer = setTimeout(run, intervalMs);
  };
  const run = async () => {
    if (stopped) return;
    if (!isVisible()) {
      schedule();
      return;
    }
    try {
      await poll();
    } finally {
      schedule();
    }
  };

  schedule();
  return () => {
    stopped = true;
    if (timer !== undefined) clearTimeout(timer);
  };
}
