import { PEER_COLORS } from "../collaboration/types";
import { useCollaborationStore } from "../stores/collaborationStore";

export function PeersPanel() {
  const siteId = useCollaborationStore((state) => state.siteId);
  const cursors = useCollaborationStore((state) => state.cursors);
  const clientCount = useCollaborationStore((state) => state.clientCount);
  const remoteEntries = Object.entries(cursors)
    .filter(([sid]) => Number(sid) !== siteId)
    .sort(([a], [b]) => Number(a) - Number(b));
  const knownClientCount = remoteEntries.length + (siteId === null ? 0 : 1);
  const displayedClientCount = clientCount || knownClientCount;

  return (
    <aside className="border p-5" aria-label="Clients">
      <div className="mb-4 flex items-baseline justify-between gap-3">
        <h2 className="text-base font-semibold">Clients</h2>
        <span className="text-sm font-semibold">
          {displayedClientCount} {displayedClientCount === 1 ? "client" : "clients"}
        </span>
      </div>

      {siteId === null ? (
        <p className="text-sm">Waiting for client id</p>
      ) : (
        <>
          <div
            className="grid grid-cols-[10px_1fr_auto] items-center gap-3 border-b py-3 text-sm"
            key={siteId}
          >
            <span
              className="h-2.5 w-2.5 rounded-full"
              style={{
                backgroundColor: PEER_COLORS[siteId % PEER_COLORS.length],
              }}
            />
            <span>
              Site #{siteId} <span className="font-semibold">(you)</span>
            </span>
            {cursors[String(siteId)] === undefined ? (
              <span />
            ) : (
              <strong className="font-semibold">@ {cursors[String(siteId)]}</strong>
            )}
          </div>

          {remoteEntries.map(([sid, pos]) => (
            <div
              className="grid grid-cols-[10px_1fr_auto] items-center gap-3 border-b py-3 text-sm"
              key={sid}
            >
              <span
                className="h-2.5 w-2.5 rounded-full"
                style={{
                  backgroundColor: PEER_COLORS[Number(sid) % PEER_COLORS.length],
                }}
              />
              <span>Site #{sid}</span>
              <strong className="font-semibold">@ {pos}</strong>
            </div>
          ))}
        </>
      )}
    </aside>
  );
}
