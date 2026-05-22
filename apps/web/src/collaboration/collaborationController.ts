import { Annotation, EditorState, StateEffect, StateField } from "@codemirror/state";
import type { ChangeSpec } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  WidgetType,
  type DecorationSet,
  type ViewUpdate,
} from "@codemirror/view";
import { Replica } from "rga-core";
import { useCollaborationStore } from "../stores/collaborationStore";
import type {
  CollaborationTransport,
  CreateCollaborationTransport,
  TransportSendResult,
} from "./collaborationTransport";
import type { Op, ServerMsg } from "./protocolSchemas";
import { colorForReplica, type ApplyOutcome, type Peer } from "./types";
import { WebSocketCollaborationTransport } from "./webSocketCollaborationTransport";

const remoteSyncAnnotation = Annotation.define<boolean>();
const setPeerCursors = StateEffect.define<Peer[]>();

const peerCursorField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(decorations, transaction) {
    let next = decorations.map(transaction.changes);
    for (const effect of transaction.effects) {
      if (effect.is(setPeerCursors)) {
        next = buildPeerDecorations(effect.value, transaction.state.doc.length);
      }
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field),
});

class CursorWidget extends WidgetType {
  constructor(
    private readonly replicaId: string,
    private readonly color: string,
  ) {
    super();
  }

  eq(other: CursorWidget): boolean {
    return this.replicaId === other.replicaId && this.color === other.color;
  }

  toDOM(): HTMLElement {
    const cursor = document.createElement("span");
    cursor.className = "peer-cursor";
    cursor.style.color = this.color;
    cursor.title = `Replica ${this.replicaId}`;
    cursor.setAttribute("aria-hidden", "true");
    return cursor;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

export class CollaborationController {
  private readonly transport: CollaborationTransport;
  private readonly replicaId = getOrCreateReplicaId();
  private readonly sessionId = createRuntimeId();
  private replica = new Replica(this.replicaId, this.sessionId);
  private editorHost: HTMLElement | null = null;
  private editorView: EditorView | null = null;
  private outboundOps: Op[] = [];
  private bufferedRemoteOps: Op[] = [];
  private stopped = true;
  private lastSyncEnabled = useCollaborationStore.getState().syncEnabled;
  private unsubscribeStore: (() => void) | null = null;

  constructor(createTransport: CreateCollaborationTransport) {
    useCollaborationStore.getState().setReplicaId(this.replicaId);
    this.syncRgaTree();
    this.transport = createTransport({
      onConnectionChange: (connection) => {
        useCollaborationStore.getState().setConnection(connection);
        if (connection === "connected") {
          this.sendHello();
          this.flushIfSyncEnabled();
        }
      },
      onMessage: (message) => this.receive(message),
    });
  }

  start() {
    if (!this.stopped) return;

    this.stopped = false;
    this.unsubscribeStore = useCollaborationStore.subscribe((state) => {
      if (state.syncEnabled !== this.lastSyncEnabled) {
        this.lastSyncEnabled = state.syncEnabled;
        if (state.syncEnabled) {
          this.flushIfSyncEnabled();
        }
      }
    });

    this.transport.connect();
  }

  stop() {
    this.stopped = true;
    this.unsubscribeStore?.();
    this.unsubscribeStore = null;
    this.transport.disconnect();
  }

  attachEditor(host: HTMLElement | null) {
    if (this.editorHost === host) return;

    this.editorView?.destroy();
    this.editorHost = host;
    this.editorView = null;

    if (!host) return;

    this.editorView = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: this.replica.text(),
        extensions: [
          peerCursorField,
          EditorView.lineWrapping,
          EditorView.contentAttributes.of({
            "aria-label": "Shared document",
          }),
          EditorView.updateListener.of((update) => this.handleEditorUpdate(update)),
        ],
      }),
    });
    this.refreshPeerCursors();
  }

  private sendHello() {
    this.warnSendFailure(
      this.transport.send({
        type: "hello",
        replica_id: this.replicaId,
        session_id: this.sessionId,
      }),
      "hello message",
    );
  }

  private flushIfSyncEnabled() {
    if (!this.isSyncEnabled()) return;

    const ops = this.outboundOps.splice(0);
    this.sendOpsOrRequeue(ops);

    if (this.bufferedRemoteOps.length > 0) {
      const outcomes = this.replica.applyRemoteBatch(this.bufferedRemoteOps.splice(0));
      this.warnRejected(outcomes);
      if (outcomes.includes("applied")) {
        this.patchEditorText(this.replica.text());
      }
    }

    this.sendPresence();
    this.updateQueueCounts();
  }

  private receive(message: ServerMsg) {
    switch (message.type) {
      case "hydrate":
        this.replica = new Replica(this.replicaId, this.sessionId);
        this.outboundOps = [];
        this.bufferedRemoteOps = [];
        this.warnRejected(this.replica.applyRemoteBatch(message.ops));
        useCollaborationStore.getState().setPresenceState(message.presence, message.clients);
        this.patchEditorText(this.replica.text());
        this.refreshPeerCursors();
        this.updateQueueCounts();
        return;
      case "ops": {
        if (!this.isSyncEnabled()) {
          this.bufferedRemoteOps.push(...message.ops);
          this.updateQueueCounts();
          return;
        }

        const outcomes = this.replica.applyRemoteBatch(message.ops);
        this.warnRejected(outcomes);
        if (outcomes.includes("applied")) {
          this.patchEditorText(this.replica.text());
        }
        return;
      }
      case "presence":
        useCollaborationStore.getState().setPresenceState(message.presence, message.clients);
        this.refreshPeerCursors();
        return;
      case "garbage_collect":
        this.applyForcedGarbageCollection();
        return;
    }
  }

  garbageCollectTombstones() {
    this.applyForcedGarbageCollection();
    if (this.transport.isOpen) {
      this.warnSendFailure(
        this.transport.send({ type: "garbage_collect" }),
        "garbage collect request",
      );
    }
  }

  private handleEditorUpdate(update: ViewUpdate) {
    if (update.transactions.some((transaction) => transaction.annotation(remoteSyncAnnotation))) {
      return;
    }

    if (update.selectionSet) {
      this.sendPresence();
    }

    if (!update.docChanged) return;

    const deletions: Array<{ from: number; to: number }> = [];
    const insertions: Array<{ from: number; text: string }> = [];

    update.changes.iterChanges((fromA, toA, fromB, _toB, inserted) => {
      if (toA > fromA) {
        deletions.push({ from: fromA, to: toA });
      }

      const text = inserted.toString();
      if (text.length > 0) {
        insertions.push({ from: fromB, text });
      }
    });

    deletions
      .sort((left, right) => right.from - left.from || right.to - left.to)
      .forEach((deletion) => {
        for (let pos = deletion.to - 1; pos >= deletion.from; pos--) {
          const op = this.replica.localDelete(pos);
          if (op) {
            this.outboundOps.push(op);
          }
        }
      });

    insertions
      .sort((left, right) => left.from - right.from)
      .forEach((insertion) => {
        let offset = 0;
        for (const ch of Array.from(insertion.text)) {
          const op = this.replica.localInsert(insertion.from + offset, ch);
          if (op) {
            this.outboundOps.push(op);
            offset++;
          }
        }
      });

    const replicaText = this.replica.text();
    const editorText = update.state.doc.toString();
    useCollaborationStore.getState().setDocumentText(replicaText);
    this.syncRgaTree();
    this.flushIfSyncEnabled();
    this.updateQueueCounts();
    if (replicaText !== editorText) {
      console.warn("RGA/editor mismatch", { replicaText, editorText });
    }
  }

  private patchEditorText(nextText: string) {
    const view = this.editorView;
    useCollaborationStore.getState().setDocumentText(nextText);
    this.syncRgaTree();
    if (!view) return;

    const patch = singleRangePatch(view.state.doc.toString(), nextText);
    const effects = [this.peerCursorEffect()];

    if (!patch) {
      view.dispatch({
        annotations: remoteSyncAnnotation.of(true),
        effects,
      });
      return;
    }

    view.dispatch({
      changes: patch,
      annotations: remoteSyncAnnotation.of(true),
      effects,
    });
  }

  private refreshPeerCursors() {
    const view = this.editorView;
    if (!view) return;

    view.dispatch({
      effects: [this.peerCursorEffect()],
    });
  }

  private peerCursorEffect() {
    const view = this.editorView;
    const docLength = view?.state.doc.length ?? 0;
    const peers: Peer[] = Object.values(useCollaborationStore.getState().presence)
      .filter((presence) => presence.replica_id !== this.replicaId)
      .map((presence) => ({
        replicaId: presence.replica_id,
        pos: Math.min(Math.max(0, presence.cursor), docLength),
      }));
    return setPeerCursors.of(peers);
  }

  private currentCursor(): number | null {
    const view = this.editorView;
    if (!view?.hasFocus) return null;
    return view.state.selection.main.head;
  }

  private sendPresence() {
    if (!this.transport.isOpen || !this.isSyncEnabled()) return;

    const cursor = this.currentCursor();
    if (cursor === null) return;

    this.warnSendFailure(
      this.transport.send({
        type: "presence",
        presence: {
          replica_id: this.replicaId,
          cursor,
        },
      }),
      "presence update",
    );
  }

  private sendOpsOrRequeue(ops: Op[]) {
    if (ops.length === 0) return;

    const result = this.transport.send({ type: "ops", ops });
    if (result.ok) return;

    if (result.reason !== "not_connected") {
      console.warn("Failed to send operation.", result.message);
    }

    if (result.reason !== "invalid_message") {
      this.outboundOps.unshift(...ops);
    }
  }

  private warnSendFailure(result: TransportSendResult, description: string) {
    if (result.ok || result.reason === "not_connected") return;

    console.warn(`Failed to send message ${description}.`, result.message);
  }

  private isSyncEnabled(): boolean {
    return useCollaborationStore.getState().syncEnabled;
  }

  private updateQueueCounts() {
    useCollaborationStore
      .getState()
      .setQueueCounts(this.outboundOps.length, this.bufferedRemoteOps.length);
  }

  private warnRejected(outcomes: ApplyOutcome[]) {
    const rejected = outcomes.filter((outcome) => outcome !== "applied" && outcome !== "duplicate");
    if (rejected.length > 0) {
      console.warn("Some RGA ops were rejected", rejected);
    }
  }

  private syncRgaTree() {
    useCollaborationStore.getState().setRgaTree(this.replica.rgaTree());
  }

  private applyForcedGarbageCollection() {
    this.replica.clearDeletedNodes();
    this.patchEditorText(this.replica.text());
  }
}

function buildPeerDecorations(peers: Peer[], docLength: number): DecorationSet {
  return Decoration.set(
    peers.map((peer) =>
      Decoration.widget({
        widget: new CursorWidget(peer.replicaId, colorForReplica(peer.replicaId)),
        side: -1,
      }).range(Math.min(peer.pos, docLength)),
    ),
    true,
  );
}

function singleRangePatch(currentText: string, nextText: string): ChangeSpec | null {
  if (currentText === nextText) return null;

  let prefix = 0;
  while (
    prefix < currentText.length &&
    prefix < nextText.length &&
    currentText[prefix] === nextText[prefix]
  ) {
    prefix++;
  }

  let currentEnd = currentText.length;
  let nextEnd = nextText.length;
  while (
    currentEnd > prefix &&
    nextEnd > prefix &&
    currentText[currentEnd - 1] === nextText[nextEnd - 1]
  ) {
    currentEnd--;
    nextEnd--;
  }

  return {
    from: prefix,
    to: currentEnd,
    insert: nextText.slice(prefix, nextEnd),
  };
}

function getOrCreateReplicaId(): string {
  const key = "nettverk.replica_id";
  const stored = localStorage.getItem(key);
  if (stored) return stored;

  const replicaId = createRuntimeId();
  localStorage.setItem(key, replicaId);
  return replicaId;
}

function createRuntimeId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

export const collaborationController = new CollaborationController(
  (handlers) => new WebSocketCollaborationTransport(handlers),
);
