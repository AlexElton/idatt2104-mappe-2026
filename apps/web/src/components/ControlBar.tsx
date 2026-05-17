import { collaborationController } from "../collaboration/collaborationController";
import { useCollaborationStore } from "../stores/collaborationStore";

export function ControlBar() {
  const syncEnabled = useCollaborationStore((state) => state.syncEnabled);
  const pendingOpsCount = useCollaborationStore((state) => state.pendingOpsCount);
  const bufferedRemoteOpsCount = useCollaborationStore((state) => state.bufferedRemoteOpsCount);
  const deletedNodeCount = useCollaborationStore(
    (state) => state.rgaTree.nodes.filter((node) => node.tombstone).length,
  );
  const setSyncEnabled = useCollaborationStore((state) => state.setSyncEnabled);
  const queuedCount = pendingOpsCount + bufferedRemoteOpsCount;

  return (
    <footer className="flex flex-col gap-4 border-t p-5 text-sm md:flex-row md:items-center md:justify-between">
      <label className="flex items-center gap-3">
        <input
          className="peer sr-only"
          type="checkbox"
          role="switch"
          checked={syncEnabled}
          onChange={(event) => setSyncEnabled(event.currentTarget.checked)}
        />
        <span
          className="relative h-6 w-11 border transition-colors after:absolute after:left-0.5 after:top-0.5 after:h-4 after:w-4 after:border after:bg-current after:transition-transform peer-checked:after:translate-x-5"
          aria-hidden="true"
        />
        <span>Sync</span>
        <strong>{syncEnabled ? "on" : "off"}</strong>
      </label>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <span>Queued local: {pendingOpsCount}</span>
        <span>Buffered remote: {bufferedRemoteOpsCount}</span>
        <strong>{queuedCount === 0 ? "Up to date" : `${queuedCount} waiting`}</strong>
        <button
          className="border px-3 py-1 font-semibold disabled:opacity-50"
          type="button"
          disabled={deletedNodeCount === 0}
          onClick={() => collaborationController.garbageCollectTombstones()}
        >
          Clear deleted ({deletedNodeCount})
        </button>
      </div>
    </footer>
  );
}
