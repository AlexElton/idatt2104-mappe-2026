import { ClientMsgSchema, ServerMsgSchema, type ClientMsg } from "./protocolSchemas";
import type {
  CollaborationTransport,
  CollaborationTransportHandlers,
  TransportSendResult,
} from "./collaborationTransport";

const RECONNECT_DELAY_MS = 2000;

export class WebSocketCollaborationTransport implements CollaborationTransport {
  private ws: WebSocket | null = null;
  private readonly intentionalClose = new WeakSet<WebSocket>();
  private reconnectTimer: number | null = null;
  private stopped = true;

  constructor(private readonly handlers: CollaborationTransportHandlers) {}

  get isOpen(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  connect(): void {
    this.stopped = false;
    this.openSocket();
  }

  disconnect(): void {
    this.stopped = true;
    this.clearReconnect();
    this.closeSocket();
    this.handlers.onConnectionChange("disconnected");
  }

  send(message: ClientMsg): TransportSendResult {
    const validation = ClientMsgSchema.safeParse(message);
    if (!validation.success) {
      const failureMessage = "Client message did not match the collaboration protocol.";
      console.warn(failureMessage, validation.error);
      return {
        ok: false,
        reason: "invalid_message",
        message: failureMessage,
      };
    }

    const ws = this.ws;
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      return {
        ok: false,
        reason: "not_connected",
        message: "Collaboration transport is not connected.",
      };
    }

    try {
      ws.send(JSON.stringify(validation.data));
      return { ok: true };
    } catch (error) {
      const failureMessage = error instanceof Error ? error.message : "WebSocket send failed.";
      console.warn("Failed to send collaboration message.", error);
      return {
        ok: false,
        reason: "send_failed",
        message: failureMessage,
      };
    }
  }

  private openSocket(): void {
    if (this.stopped) return;

    this.clearReconnect();
    this.closeSocket();
    this.handlers.onConnectionChange("connecting");

    const ws = new WebSocket(this.websocketUrl());
    this.ws = ws;

    ws.onopen = () => {
      if (this.ws !== ws) return;
      this.handlers.onConnectionChange("connected");
    };

    ws.onclose = () => {
      if (this.ws !== ws) return;

      this.ws = null;
      this.handlers.onConnectionChange("disconnected");
      if (this.intentionalClose.has(ws)) return;

      this.scheduleReconnect();
    };

    ws.onerror = () => {
      if (this.ws !== ws) return;
      this.handlers.onConnectionChange("error");
    };

    ws.onmessage = (event: MessageEvent<unknown>) => {
      if (this.ws !== ws) return;
      this.receive(event.data);
    };
  }

  private receive(data: unknown): void {
    if (typeof data !== "string") {
      console.warn("Invalid collaboration server message: expected a text frame.");
      return;
    }

    let decoded: unknown;
    try {
      decoded = JSON.parse(data) as unknown;
    } catch (error) {
      console.warn("Invalid collaboration server message: failed to parse JSON.", error);
      return;
    }

    const validation = ServerMsgSchema.safeParse(decoded);
    if (!validation.success) {
      console.warn("Invalid collaboration server message.", validation.error);
      return;
    }

    this.handlers.onMessage(validation.data);
  }

  private closeSocket(): void {
    const ws = this.ws;
    this.ws = null;
    if (!ws) return;

    this.intentionalClose.add(ws);
    ws.close();
  }

  private scheduleReconnect(): void {
    if (this.stopped) return;

    this.clearReconnect();
    this.reconnectTimer = window.setTimeout(() => this.openSocket(), RECONNECT_DELAY_MS);
  }

  private clearReconnect(): void {
    if (this.reconnectTimer === null) return;

    window.clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
  }

  private websocketUrl(): string {
    const proto = location.protocol === "https:" ? "wss" : "ws";
    return `${proto}://${location.host}/ws`;
  }
}
