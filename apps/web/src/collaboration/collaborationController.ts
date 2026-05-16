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
import { SocketClient } from "./socketClient";
import { colorForReplica, type ApplyOutcome, type Op, type Peer, type ServerMsg } from "./types";

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

const editorTheme = EditorView.theme({
  "&": {
    minHeight: "20rem",
    height: "100%",
    backgroundColor: "transparent",
  },
  "&.cm-focused": {
    outline: "none",
  },
  ".cm-scroller": {
    fontFamily: "inherit",
    lineHeight: "1.625",
    overflow: "auto",
  },
  ".cm-content": {
    minHeight: "20rem",
    padding: "1.25rem",
    whiteSpace: "pre-wrap",
    wordBreak: "break-word",
    caretColor: "currentColor",
  },
  ".cm-line": {
    padding: "0",
  },
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

class CollaborationController {
  private readonly socket: SocketClient;
  private readonly replicaId = getOrCreateReplicaId();
  private readonly sessionId = createRuntimeId();
  private replica = new Replica(this.replicaId, this.sessionId);
  private editorHost: HTMLElement | null = null;
  private editorView: EditorView | null = null;
  private outboundOps: Op[] = [];
  private reconnectTimer: number | null = null;
  private syncTimer: number | null = null;
  private countdownTimer: number | null = null;
  private stopped = true;
  private lastIntervalMs = useCollaborationStore.getState().syncIntervalMs;
  private unsubscribeStore: (() => void) | null = null;

  constructor() {
    useCollaborationStore.getState().setReplicaId(this.replicaId);
    this.socket = new SocketClient({
      onConnectionChange: (connection) => {
        useCollaborationStore.getState().setConnection(connection);
        if (connection === "connected") {
          this.sendHello();
        }
      },
      onMessage: (message) => this.receive(message),
      onClose: () => this.scheduleReconnect(),
    });
  }

  start() {
    if (!this.stopped) return;

    this.stopped = false;
    this.unsubscribeStore = useCollaborationStore.subscribe((state) => {
      if (state.syncIntervalMs !== this.lastIntervalMs) {
        this.lastIntervalMs = state.syncIntervalMs;
        this.scheduleSync();
      }
    });

    this.connect();
    this.scheduleSync();
    this.startCountdown();
  }

  stop() {
    this.stopped = true;
    this.unsubscribeStore?.();
    this.unsubscribeStore = null;
    this.clearReconnect();
    this.clearSync();
    this.clearCountdown();
    this.socket.disconnect();
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
          editorTheme,
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

  private connect() {
    this.clearReconnect();
    const proto = location.protocol === "https:" ? "wss" : "ws";
    this.socket.connect(`${proto}://${location.host}/ws`);
  }

  private sendHello() {
    this.socket.send({
      type: "hello",
      replica_id: this.replicaId,
      session_id: this.sessionId,
    });
  }

  private scheduleReconnect() {
    if (this.stopped) return;
    this.clearReconnect();
    this.reconnectTimer = window.setTimeout(() => this.connect(), 2000);
  }

  private scheduleSync() {
    this.clearSync();
    const intervalMs = useCollaborationStore.getState().syncIntervalMs;
    this.syncTimer = window.setInterval(() => this.syncNow(), intervalMs);
    useCollaborationStore.getState().setCountdownMs(intervalMs);
  }

  private startCountdown() {
    this.clearCountdown();
    this.countdownTimer = window.setInterval(() => {
      if (this.socket.isOpen) {
        useCollaborationStore.getState().tickCountdown(100);
      }
    }, 100);
  }

  private syncNow() {
    if (!this.socket.isOpen) return;

    const ops = this.outboundOps.splice(0);
    if (ops.length > 0) {
      this.socket.send({ type: "ops", ops });
    }

    const cursor = this.currentCursor();
    if (cursor !== null) {
      this.socket.send({
        type: "presence",
        presence: {
          replica_id: this.replicaId,
          cursor,
        },
      });
    }

    useCollaborationStore
      .getState()
      .setCountdownMs(useCollaborationStore.getState().syncIntervalMs);
  }

  private receive(message: ServerMsg) {
    switch (message.type) {
      case "hydrate":
        this.replica = new Replica(this.replicaId, this.sessionId);
        this.outboundOps = [];
        this.warnRejected(this.replica.applyRemoteBatch(message.ops));
        useCollaborationStore.getState().setPresenceState(message.presence, message.clients);
        this.patchEditorText(this.replica.text());
        this.refreshPeerCursors();
        return;
      case "ops": {
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
    }
  }

  private handleEditorUpdate(update: ViewUpdate) {
    if (!update.docChanged) return;
    if (update.transactions.some((transaction) => transaction.annotation(remoteSyncAnnotation))) {
      return;
    }

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
    if (replicaText !== editorText) {
      console.warn("RGA/editor mismatch", { replicaText, editorText });
    }
  }

  private patchEditorText(nextText: string) {
    const view = this.editorView;
    useCollaborationStore.getState().setDocumentText(nextText);
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

  private warnRejected(outcomes: ApplyOutcome[]) {
    const rejected = outcomes.filter((outcome) => outcome !== "applied" && outcome !== "duplicate");
    if (rejected.length > 0) {
      console.warn("Some RGA ops were rejected", rejected);
    }
  }

  private clearReconnect() {
    if (this.reconnectTimer !== null) {
      window.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private clearSync() {
    if (this.syncTimer !== null) {
      window.clearInterval(this.syncTimer);
      this.syncTimer = null;
    }
  }

  private clearCountdown() {
    if (this.countdownTimer !== null) {
      window.clearInterval(this.countdownTimer);
      this.countdownTimer = null;
    }
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

export const collaborationController = new CollaborationController();
