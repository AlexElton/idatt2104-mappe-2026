import type { ClientMsg, ConnectionStatus, ServerMsg } from "./types";

type SocketClientOptions = {
  onConnectionChange: (connection: ConnectionStatus) => void;
  onMessage: (message: ServerMsg) => void;
  onClose: () => void;
};

export class SocketClient {
  private ws: WebSocket | null = null;
  private intentionalClose = new WeakSet<WebSocket>();

  constructor(private readonly options: SocketClientOptions) {}

  get isOpen(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  connect(url: string) {
    this.disconnect();
    this.options.onConnectionChange("connecting");

    const ws = new WebSocket(url);
    this.ws = ws;

    ws.onopen = () => {
      if (this.ws !== ws) return;
      this.options.onConnectionChange("connected");
    };

    ws.onclose = () => {
      if (this.ws !== ws) return;

      this.ws = null;
      this.options.onConnectionChange("disconnected");
      if (this.intentionalClose.has(ws)) return;

      this.options.onClose();
    };

    ws.onerror = () => {
      if (this.ws !== ws) return;
      this.options.onConnectionChange("error");
    };

    ws.onmessage = (event: MessageEvent<string>) => {
      if (this.ws !== ws) return;
      this.options.onMessage(JSON.parse(event.data) as ServerMsg);
    };
  }

  disconnect() {
    const ws = this.ws;
    this.ws = null;
    if (!ws) return;

    this.intentionalClose.add(ws);
    ws.close();
  }

  send(message: ClientMsg) {
    if (!this.isOpen) return;
    this.ws?.send(JSON.stringify(message));
  }
}
