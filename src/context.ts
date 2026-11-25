import { createContext, useContext } from "react";
import type { Snapshot } from "./types";

export type View =
  "projects" | "search" | "sources" | "profile" | "agents" | "settings";
export interface KnowledgeContext {
  data: Snapshot;
  refresh: () => Promise<void>;
  notify: (message: string, error?: boolean) => void;
  view: View;
  navigate: (view: View) => void;
  projectId: string;
  selectProject: (id: string) => void;
  openSource: (id: string, chunkId?: string) => void;
  addRoot: () => void;
}
export const Context = createContext<KnowledgeContext | null>(null);
export function useKnowledge() {
  const value = useContext(Context);
  if (!value) throw new Error("知识库尚未就绪");
  return value;
}
