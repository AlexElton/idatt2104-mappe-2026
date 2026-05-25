/**
 * This file is a light weight wrapper around the Rust crate types.
 */

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

export type RgaTreeNode = {
  index: number;
  visible_index: number | null;
  value: string;
  tombstone: boolean;
  id: OperationId;
  left: OperationId | null;
  next: OperationId | null;
  deleted_by: OperationId | null;
};

export type RgaTree = {
  text: string;
  nodes: RgaTreeNode[];
};

type RawReplicaApi = {
  localInsert(pos: number, value: string): unknown;
  localDelete(pos: number): unknown;
  applyRemote(op: Op): ApplyOutcome;
  applyRemoteBatch(ops: Op[]): ApplyOutcome[];
  text(): string;
  hydrationOps(): Op[];
  rgaTree(): RgaTree;
  clearDeletedNodes(): number;
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

  rgaTree(): RgaTree {
    return this.inner.rgaTree();
  }

  clearDeletedNodes(): number {
    return this.inner.clearDeletedNodes();
  }
}

function normalizeOp(value: unknown): Op | undefined {
  if (value === undefined || value === null) return undefined;
  return value as Op;
}
