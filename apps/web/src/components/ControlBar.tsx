import { useCollaborationStore } from "../stores/collaborationStore";

export function ControlBar() {
  const syncIntervalMs = useCollaborationStore((state) => state.syncIntervalMs);
  const countdownMs = useCollaborationStore((state) => state.countdownMs);
  const setSyncIntervalMs = useCollaborationStore(
    (state) => state.setSyncIntervalMs,
  );

  return (
    <footer className="flex flex-col gap-4 border-t p-5 text-sm md:flex-row md:items-center md:justify-between">
      <label className="grid min-w-0 grid-cols-[1fr_auto] items-center gap-3 md:grid-cols-[auto_auto_minmax(180px,320px)]">
        <span>Sync interval</span>
        <strong>{syncIntervalMs}ms</strong>
        <input
          className="col-span-2 w-full md:col-span-1"
          type="range"
          min="100"
          max="5000"
          step="100"
          value={syncIntervalMs}
          onChange={(event) =>
            setSyncIntervalMs(Number(event.currentTarget.value))
          }
        />
      </label>
      <div>Next sync: {(countdownMs / 1000).toFixed(1)}s</div>
    </footer>
  );
}
