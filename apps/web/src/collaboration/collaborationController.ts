import { useCollaborationStore } from "../stores/collaborationStore";
import { buildEditorHtml, getCaretOffset, getPlainText, SENTINEL, setCaretOffset } from "./editorDom";
import { SocketClient } from "./socketClient";
import type { Peer, ServerMsg } from "./types";

class CollaborationController {
  private readonly socket: SocketClient;
  private editor: HTMLElement | null = null;
  private reconnectTimer: number | null = null;
  private syncTimer: number | null = null;
  private countdownTimer: number | null = null;
  private stopped = true;
  private lastIntervalMs = useCollaborationStore.getState().syncIntervalMs;
  private lastRemoteRevision = useCollaborationStore.getState().remoteRevision;
  private unsubscribeStore: (() => void) | null = null;

  constructor() {
    this.socket = new SocketClient({
      onConnectionChange: (connection) => useCollaborationStore.getState().setConnection(connection),
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

      if (state.remoteRevision !== this.lastRemoteRevision) {
        this.lastRemoteRevision = state.remoteRevision;
        this.renderEditorState();
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

  attachEditor(editor: HTMLElement | null) {
    this.editor = editor;
    this.renderEditorState();
  }

  private connect() {
    this.clearReconnect();
    const proto = location.protocol === "https:" ? "wss" : "ws";
    this.socket.connect(`${proto}://${location.host}/ws`);
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
    if (!this.socket.isOpen || !this.editor) return;

    const currentText = getPlainText(this.editor).replaceAll(SENTINEL, "");
    useCollaborationStore.getState().stageLocalText(currentText);

    const selection = window.getSelection();
    const hasFocus =
      document.activeElement === this.editor &&
      Boolean(selection?.rangeCount) &&
      selection !== null &&
      this.editor.contains(selection.getRangeAt(0).endContainer);

    const ops = useCollaborationStore.getState().takePendingOps();
    useCollaborationStore.getState().setCountdownMs(useCollaborationStore.getState().syncIntervalMs);
    if (ops.length === 0) return;

    this.socket.send({
      type: "ops",
      ops,
      ...(hasFocus ? { cursor: getCaretOffset(this.editor) } : {}),
    });
  }

  private receive(message: ServerMsg) {
    if (message.type === "init") {
      useCollaborationStore.getState().receiveInit(message);
      return;
    }

    useCollaborationStore.getState().receiveState(message);
  }

  private renderEditorState() {
    const editor = this.editor;
    if (!editor) return;

    const { documentText, cursors, siteId, replaceEditorText } = useCollaborationStore.getState();
    const localText = getPlainText(editor).replaceAll(SENTINEL, "");
    const textToRender = replaceEditorText ? documentText : localText;
    const focused = document.activeElement === editor;
    const caretPos = focused ? getCaretOffset(editor) : null;
    const peers: Peer[] = Object.entries(cursors)
      .filter(([sid]) => Number(sid) !== siteId)
      .map(([sid, pos]) => ({ sid: Number(sid), pos: Math.min(Number(pos), textToRender.length) }));

    editor.innerHTML = buildEditorHtml(textToRender, peers);
    if (focused && caretPos !== null) {
      setCaretOffset(editor, Math.min(caretPos, textToRender.length));
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

export const collaborationController = new CollaborationController();
