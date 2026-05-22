export type { ApplyOutcome, RgaTree, RgaTreeNode } from "rga-core";
export type { OperationId } from "./protocolSchemas";

export const PEER_COLORS = ["#4ec9b0", "#ce9178", "#dcdcaa", "#9cdcfe", "#c586c0", "#f44747"];

export type ConnectionStatus = "connecting" | "connected" | "disconnected" | "error";

export type Peer = {
  replicaId: string;
  pos: number;
};

export function colorForReplica(replicaId: string): string {
  let hash = 0;
  for (let index = 0; index < replicaId.length; index++) {
    hash = (hash * 31 + replicaId.charCodeAt(index)) >>> 0;
  }
  return PEER_COLORS[hash % PEER_COLORS.length];
}

export function shortReplicaId(replicaId: string): string {
  return replicaId.length <= 8 ? replicaId : replicaId.slice(0, 8);
}
