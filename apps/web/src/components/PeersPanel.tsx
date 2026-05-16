import { colorForReplica, shortReplicaId } from "../collaboration/types";
import { useCollaborationStore } from "../stores/collaborationStore";

export function PeersPanel() {
  const replicaId = useCollaborationStore((state) => state.replicaId);
  const presence = useCollaborationStore((state) => state.presence);
  const clientCount = useCollaborationStore((state) => state.clientCount);
  const remoteEntries = Object.values(presence)
    .filter((peer) => peer.replica_id !== replicaId)
    .sort((left, right) => left.replica_id.localeCompare(right.replica_id));
  const knownClientCount = remoteEntries.length + (replicaId === null ? 0 : 1);
  const displayedClientCount = clientCount || knownClientCount;
  const ownCursor = replicaId === null ? undefined : presence[replicaId]?.cursor;

  return (
    <aside className="border p-5" aria-label="Clients">
      <div className="mb-4 flex items-baseline justify-between gap-3">
        <h2 className="text-base font-semibold">Clients</h2>
        <span className="text-sm font-semibold">
          {displayedClientCount} {displayedClientCount === 1 ? "client" : "clients"}
        </span>
      </div>

      {replicaId === null ? (
        <p className="text-sm">Waiting for replica</p>
      ) : (
        <>
          <div
            className="grid grid-cols-[10px_1fr_auto] items-center gap-3 border-b py-3 text-sm"
            key={replicaId}
          >
            <span
              className="h-2.5 w-2.5 rounded-full"
              style={{
                backgroundColor: colorForReplica(replicaId),
              }}
            />
            <span>
              {shortReplicaId(replicaId)} <span className="font-semibold">(you)</span>
            </span>
            {ownCursor === undefined ? (
              <span />
            ) : (
              <strong className="font-semibold">@ {ownCursor}</strong>
            )}
          </div>

          {remoteEntries.map((peer) => (
            <div
              className="grid grid-cols-[10px_1fr_auto] items-center gap-3 border-b py-3 text-sm"
              key={peer.replica_id}
            >
              <span
                className="h-2.5 w-2.5 rounded-full"
                style={{
                  backgroundColor: colorForReplica(peer.replica_id),
                }}
              />
              <span>{shortReplicaId(peer.replica_id)}</span>
              <strong className="font-semibold">@ {peer.cursor}</strong>
            </div>
          ))}
        </>
      )}
    </aside>
  );
}
