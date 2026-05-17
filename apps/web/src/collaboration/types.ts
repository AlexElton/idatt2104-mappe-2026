import type { Op } from "rga-core";

export type { ApplyOutcome, Op, OperationId, RgaTree, RgaTreeNode } from "rga-core";

export const PEER_COLORS = ["#4ec9b0", "#ce9178", "#dcdcaa", "#9cdcfe", "#c586c0", "#f44747"];

export type ConnectionStatus = "connecting" | "connected" | "disconnected" | "error";

export type Presence = {
  replica_id: string;
  cursor: number;
};

export type HelloMsg = {
  type: "hello";
  replica_id: string;
  session_id: string;
};

export type ClientOpsMsg = {
  type: "ops";
  ops: Op[];
};

export type ClientPresenceMsg = {
  type: "presence";
  presence: Presence;
};

export type ClientGarbageCollectMsg = {
  type: "garbage_collect";
};

export type ClientMsg = HelloMsg | ClientOpsMsg | ClientPresenceMsg | ClientGarbageCollectMsg;

export type HydrateMsg = {
  type: "hydrate";
  ops: Op[];
  presence: Record<string, Presence>;
  clients: number;
};

export type RemoteOpsMsg = {
  type: "ops";
  ops: Op[];
};

export type PresenceStateMsg = {
  type: "presence";
  presence: Record<string, Presence>;
  clients: number;
};

export type GarbageCollectMsg = {
  type: "garbage_collect";
  removed: number;
};

export type ServerMsg = HydrateMsg | RemoteOpsMsg | PresenceStateMsg | GarbageCollectMsg;

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
