import { create } from "zustand";
import { computeOps } from "../collaboration/rgaActions";
import type { ClientOp, ConnectionStatus, InitMsg, StateMsg } from "../collaboration/types";

type CollaborationState = {
  siteId: number | null;
  clientCount: number;
  cursors: Record<string, number>;
  syncIntervalMs: number;
  countdownMs: number;
  connection: ConnectionStatus;
  documentText: string;
  pendingOps: ClientOp[];
  remoteRevision: number;
  replaceEditorText: boolean;
  setConnection: (connection: ConnectionStatus) => void;
  setSyncIntervalMs: (syncIntervalMs: number) => void;
  setCountdownMs: (countdownMs: number) => void;
  tickCountdown: (deltaMs: number) => void;
  receiveInit: (msg: InitMsg) => void;
  receiveState: (msg: StateMsg) => void;
  stageLocalText: (text: string) => void;
  takePendingOps: () => ClientOp[];
};

export const useCollaborationStore = create<CollaborationState>((set, get) => ({
  siteId: null,
  clientCount: 0,
  cursors: {},
  syncIntervalMs: 1000,
  countdownMs: 1000,
  connection: "connecting",
  documentText: "",
  pendingOps: [],
  remoteRevision: 0,
  replaceEditorText: true,
  setConnection: (connection) => set({ connection }),
  setSyncIntervalMs: (syncIntervalMs) => set({ syncIntervalMs, countdownMs: syncIntervalMs }),
  setCountdownMs: (countdownMs) => set({ countdownMs }),
  tickCountdown: (deltaMs) => set((state) => ({ countdownMs: Math.max(0, state.countdownMs - deltaMs) })),
  receiveInit: (msg) =>
    set((state) => ({
      siteId: msg.site_id,
      clientCount: Object.keys(msg.cursors || {}).length || 1,
      cursors: msg.cursors || {},
      documentText: msg.text,
      pendingOps: [],
      remoteRevision: state.remoteRevision + 1,
      replaceEditorText: true,
    })),
  receiveState: (msg) =>
    set((state) => {
      const textChanged = msg.text !== state.documentText;
      return {
        clientCount: msg.clients,
        cursors: msg.cursors || {},
        documentText: textChanged ? msg.text : state.documentText,
        pendingOps: textChanged ? [] : state.pendingOps,
        remoteRevision: state.remoteRevision + 1,
        replaceEditorText: textChanged,
      };
    }),
  stageLocalText: (text) =>
    set((state) => {
      const newOps = computeOps(state.documentText, text);
      return {
        documentText: text,
        pendingOps: [...state.pendingOps, ...newOps],
      };
    }),
  takePendingOps: () => {
    const ops = get().pendingOps;
    set({ pendingOps: [] });
    return ops;
  },
}));
