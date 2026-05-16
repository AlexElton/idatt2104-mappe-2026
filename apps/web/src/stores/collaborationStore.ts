import { create } from "zustand";
import type { ConnectionStatus, Presence } from "../collaboration/types";

type CollaborationState = {
  replicaId: string | null;
  clientCount: number;
  presence: Record<string, Presence>;
  syncIntervalMs: number;
  countdownMs: number;
  connection: ConnectionStatus;
  documentText: string;
  setConnection: (connection: ConnectionStatus) => void;
  setReplicaId: (replicaId: string) => void;
  setSyncIntervalMs: (syncIntervalMs: number) => void;
  setCountdownMs: (countdownMs: number) => void;
  tickCountdown: (deltaMs: number) => void;
  setDocumentText: (documentText: string) => void;
  setPresenceState: (presence: Record<string, Presence>, clientCount: number) => void;
};

export const useCollaborationStore = create<CollaborationState>((set) => ({
  replicaId: null,
  clientCount: 0,
  presence: {},
  syncIntervalMs: 1000,
  countdownMs: 1000,
  connection: "connecting",
  documentText: "",
  setConnection: (connection) => set({ connection }),
  setReplicaId: (replicaId) => set({ replicaId }),
  setSyncIntervalMs: (syncIntervalMs) => set({ syncIntervalMs, countdownMs: syncIntervalMs }),
  setCountdownMs: (countdownMs) => set({ countdownMs }),
  tickCountdown: (deltaMs) =>
    set((state) => ({ countdownMs: Math.max(0, state.countdownMs - deltaMs) })),
  setDocumentText: (documentText) => set({ documentText }),
  setPresenceState: (presence, clientCount) => set({ presence, clientCount }),
}));
