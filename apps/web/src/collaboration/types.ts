export const PEER_COLORS = ["#4ec9b0", "#ce9178", "#dcdcaa", "#9cdcfe", "#c586c0", "#f44747"];

export type ConnectionStatus = "connecting" | "connected" | "disconnected" | "error";

export type ClientOp = {
  op: "insert" | "delete";
  pos: number;
  char?: string;
};

export type ClientOpsMsg = {
  type: "ops";
  ops: ClientOp[];
  cursor?: number;
};

export type InitMsg = {
  type: "init";
  site_id: number;
  text: string;
  cursors: Record<string, number>;
};

export type StateMsg = {
  type: "state";
  text: string;
  cursors: Record<string, number>;
  clients: number;
};

export type ServerMsg = InitMsg | StateMsg;

export type Peer = {
  sid: number;
  pos: number;
};
