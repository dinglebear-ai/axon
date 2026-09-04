export function TailControls({
  following,
  paused,
  onFollow,
  onPause,
  onCancel,
  onResume,
}: {
  following: boolean;
  paused: boolean;
  onFollow: () => void;
  onPause: () => void;
  onCancel: () => void;
  onResume: () => void;
}) {
  return (
    <fieldset className="cortex-tail-controls">
      <legend>Live tail controls</legend>
      {!following ? (
        <button type="button" onClick={onFollow}>
          Follow live
        </button>
      ) : (
        <>
          <button type="button" onClick={onPause}>
            Pause
          </button>
          <button type="button" onClick={onCancel}>
            Cancel tail
          </button>
        </>
      )}
      {paused && (
        <button type="button" onClick={onResume}>
          Resume from cursor
        </button>
      )}
    </fieldset>
  );
}
