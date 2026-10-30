import { create } from "zustand";
import { ipc, type ModelConfig, type ProjectSnapshot } from "@/lib/ipc";

export type WorkspaceMode = "seed" | "studio" | "map" | "audit";
export type NavKey =
  | "overview"
  | "studio"
  | "agent"
  | "seed"
  | "canon"
  | "characters"
  | "knowledge"
  | "timeline"
  | "promises"
  | "wiki"
  | "audit";

type AppStore = {
  ready: boolean;
  project: ProjectSnapshot | null;
  model: ModelConfig | null;
  nav: NavKey;
  activeMode: WorkspaceMode;
  toast: string | null;
  /** 从其它页面交给 Agent 的任务草稿，Agent 输入框打开时取用。 */
  agentDraft: string | null;
  setAgentDraft: (draft: string | null) => void;
  setProject: (project: ProjectSnapshot | null) => void;
  setModel: (model: ModelConfig | null) => void;
  setNav: (nav: NavKey) => void;
  setMode: (mode: WorkspaceMode) => void;
  notify: (message: string | null) => void;
  refreshProject: () => Promise<void>;
};

export const useAppStore = create<AppStore>((set) => ({
  ready: false,
  project: null,
  model: null,
  nav: "overview",
  activeMode: "seed",
  toast: null,
  agentDraft: null,
  setAgentDraft: (agentDraft) => set({ agentDraft }),
  setProject: (project) => set({ project }),
  setModel: (model) => set({ model }),
  setNav: (nav) => set({ nav }),
  setMode: (activeMode) => set({ activeMode }),
  notify: (toast) => set({ toast }),
  refreshProject: async () => {
    const project = await ipc.projectState();
    if (project) set({ project });
  },
}));
