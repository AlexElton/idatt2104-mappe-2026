import type { ClientMsg, ServerMsg } from "./protocolSchemas";
import type { ConnectionStatus } from "./types";

export type CollaborationTransportHandlers = {
  onConnectionChange: (connection: ConnectionStatus) => void;
  onMessage: (message: ServerMsg) => void;
};

export type TransportSendFailureReason = "not_connected" | "invalid_message" | "send_failed";

export type TransportSendResult =
  | { ok: true }
  | {
      ok: false;
      reason: TransportSendFailureReason;
      message: string;
    };

export interface CollaborationTransport {
  readonly isOpen: boolean;
  connect(): void;
  disconnect(): void;
  send(message: ClientMsg): TransportSendResult;
}

export type CreateCollaborationTransport = (
  handlers: CollaborationTransportHandlers,
) => CollaborationTransport;
