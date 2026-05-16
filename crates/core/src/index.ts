import { RawReplica } from "../pkg/rga_core.js";

export type OperationId = {
  session_id: string;
  replica_id: string;
  lamport: number;
  seq: number;
};

export type Op =
  | {
      type: "Insert";
      left: OperationId | null;
      value: string;
      id: OperationId;
    }
  | {
      type: "Delete";
      target: OperationId;
      id: OperationId;
    };

export type ApplyOutcome = "applied" | "duplicate" | "missing_dependency" | "invalid";

type RawReplicaApi = {
  localInsert(pos: number, value: string): unknown;
  localDelete(pos: number): unknown;
  applyRemote(op: Op): ApplyOutcome;
  applyRemoteBatch(ops: Op[]): ApplyOutcome[];
  text(): string;
  hydrationOps(): Op[];
};

export class Replica {
  private readonly inner: RawReplicaApi;

  constructor(replicaId: string, sessionId: string) {
    this.inner = new RawReplica(replicaId, sessionId) as RawReplicaApi;
  }

  localInsert(pos: number, value: string): Op | undefined {
    return normalizeOp(this.inner.localInsert(pos, value));
  }

  localDelete(pos: number): Op | undefined {
    return normalizeOp(this.inner.localDelete(pos));
  }

  applyRemote(op: Op): ApplyOutcome {
    return this.inner.applyRemote(op);
  }

  applyRemoteBatch(ops: Op[]): ApplyOutcome[] {
    return this.inner.applyRemoteBatch(ops);
  }

  text(): string {
    return this.inner.text();
  }

  hydrationOps(): Op[] {
    return this.inner.hydrationOps();
  }
}

function normalizeOp(value: unknown): Op | undefined {
  if (value === undefined || value === null) return undefined;
  return value as Op;
}
