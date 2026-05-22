import { create } from "zustand";
import type { Presence } from "../collaboration/protocolSchemas";
import type { ConnectionStatus, RgaTree } from "../collaboration/types";

type CollaborationState = {
  replicaId: string | null;
  clientCount: number;
  presence: Record<string, Presence>;
  syncEnabled: boolean;
  pendingOpsCount: number;
  bufferedRemoteOpsCount: number;
  connection: ConnectionStatus;
  documentText: string;
  rgaTree: RgaTree;
  setConnection: (connection: ConnectionStatus) => void;
  setReplicaId: (replicaId: string) => void;
  setSyncEnabled: (syncEnabled: boolean) => void;
  setQueueCounts: (pendingOpsCount: number, bufferedRemoteOpsCount: number) => void;
  setDocumentText: (documentText: string) => void;
  setRgaTree: (rgaTree: RgaTree) => void;
  setPresenceState: (presence: Record<string, Presence>, clientCount: number) => void;
};

export const useCollaborationStore = create<CollaborationState>((set) => ({
  replicaId: null,
  clientCount: 0,
  presence: {},
  syncEnabled: true,
  pendingOpsCount: 0,
  bufferedRemoteOpsCount: 0,
  connection: "connecting",
  documentText: "",
  rgaTree: { text: "", nodes: [] },
  setConnection: (connection) => set({ connection }),
  setReplicaId: (replicaId) => set({ replicaId }),
  setSyncEnabled: (syncEnabled) => set({ syncEnabled }),
  setQueueCounts: (pendingOpsCount, bufferedRemoteOpsCount) =>
    set({ pendingOpsCount, bufferedRemoteOpsCount }),
  setDocumentText: (documentText) => set({ documentText }),
  setRgaTree: (rgaTree) => set({ rgaTree }),
  setPresenceState: (presence, clientCount) => set({ presence, clientCount }),
}));
