export interface Appearance {
  theme: "light" | "dark" | "system";
  accent: string;
  font_size: 13 | 14 | 16 | 18;
  reading_font: "sans" | "serif";
  density: "comfortable" | "compact";
  sidebar_collapsed: boolean;
}

export interface Location {
  label: string;
  page?: number;
  line_start?: number;
  line_end?: number;
  sheet?: string;
  cells?: string;
}
export interface Evidence {
  document_id: string;
  version: string;
  title: string;
  chunk_id?: string;
  location?: Location;
}
export interface Root {
  id: string;
  path: string;
  name: string;
  policy: "local" | "company" | "agents";
  enabled: boolean;
  exclusions: string[];
  last_scan: string | null;
  error: string | null;
}
export interface Decision {
  date: string;
  text: string;
}
export interface Project {
  id: string;
  name: string;
  root_path: string;
  goal: string;
  approach: string;
  decisions: Decision[];
  summary_evidence: Evidence[];
  updated_at: string;
  summary_stale: boolean;
}
export interface Document {
  id: string;
  path: string;
  title: string;
  root_id: string;
  project_id: string;
  extension: string;
  version: string;
  size: number;
  modified_ms: number;
  updated_at: string;
  status: string;
  error: string | null;
  chunks: number;
  ocr: boolean;
}
export interface Chunk {
  id: string;
  document_id: string;
  ordinal: number;
  text: string;
  location: Location;
}
export interface SearchHit {
  chunk_id: string;
  document_id: string;
  title: string;
  project_id: string;
  project_name: string;
  path: string;
  version: string;
  updated_at: string;
  text: string;
  location: Location;
  score: number;
}
export interface SearchResult {
  answer?: string | null;
  hits: SearchHit[];
  mode: string;
  notice?: string | null;
  model?: string;
}
export interface Memory {
  id: string;
  content: string;
  category: string;
  status: string;
  project_id: string | null;
  evidence: Evidence[];
  origin: string;
  created_at: string;
  updated_at: string;
  history: unknown[];
}
export interface AgentConnection {
  id: string;
  name: string;
  kind: string;
  enabled: boolean;
  project_ids: string[];
  include_profile: boolean;
  can_propose: boolean;
  created_at: string;
  last_used: string | null;
  config_path: string | null;
}
export interface Provider {
  base_url: string;
  model: string;
  protocol: string;
  has_key: boolean;
  endpoint: string;
}
export interface Settings {
  schema_version: number;
  organization: string;
  chat: Provider;
  embedding: Provider;
  embedding_mode: "auto" | "local" | "disabled";
  rerank: Provider;
  rerank_enabled: boolean;
  auto_summarize: boolean;
  ocr_enabled: boolean;
  onboarding_complete: boolean;
  paused: boolean;
  max_file_mb: number;
  reconcile_seconds: number;
  min_free_disk_gb: number;
  max_index_gb: number;
  indexing_profile: "low" | "balanced" | "performance";
  auto_local_chunk_limit: number;
  api_embedding_batch_size: number;
}
export interface StorageInfo {
  data_directory: string;
  install_directory: string;
  available_bytes: number;
  total_bytes: number;
  items: { name: string; bytes: number }[];
}
export interface ActivityEvent {
  id: number;
  created_at: string;
  kind: string;
  message: string;
  details: Record<string, unknown>;
}
export interface Snapshot {
  version: string;
  onboarding?: { version: number; status: "new" | "skipped" | "completed" };
  roots: Root[];
  projects: Project[];
  documents: Document[];
  memories: Memory[];
  connections: AgentConnection[];
  stats: {
    documents: number;
    chunks: number;
    vectors: number;
    pending: number;
    projects: number;
    ready: number;
    errors: number;
    empty: number;
    indexing: number;
    initialized: boolean;
    discovered: number;
    cleaning?: number;
  };
  settings: Settings;
  events: ActivityEvent[];
  last_indexed: string | null;
  last_scan: string | null;
  scanning: boolean;
  embedding_busy: boolean;
  embedding_available: boolean;
  embedding_model: string;
  semantic_error: string | null;
  data_directory: string;
  appearance: Appearance;
  format_warnings: {
    root_id: string;
    root_name: string;
    extension: string;
    count: number;
  }[];
  semantic_progress: {
    mode: "disabled" | "api" | "local";
    model: string;
    total_chunks: number;
    completed_vectors: number;
    counts_ready: boolean;
    vectors_per_second: number | null;
    estimated_seconds: number | null;
    protected: boolean;
    notice: string | null;
  };
  storage: {
    available_bytes: number;
    index_bytes: number;
    quota_bytes?: number;
    reusable_bytes?: number;
    wal_bytes: number;
    reserve_bytes: number;
    limit_bytes: number;
    blocked: boolean;
    reason: string | null;
  };
}
export interface SourceResponse {
  document: Document;
  chunks: Chunk[];
  total: number;
  offset: number;
}
export interface ConnectionConfig {
  connection: AgentConnection;
  name: string;
  server: { command: string; args: string[]; env: Record<string, string> };
  config: Record<string, unknown>;
  http: { url: string; headers: Record<string, string> };
  instructions: string;
}

declare global {
  interface Window {
    knowledge?: {
      initialAppearance?: Appearance;
      request: (
        method: string,
        path: string,
        body?: unknown,
      ) => Promise<unknown>;
      chooseDirectory: () => Promise<string | null>;
      openSource: (id: string) => Promise<void>;
      revealData: () => Promise<void>;
      storageInfo: () => Promise<StorageInfo>;
      prepareStorageMove: () => Promise<{
        path: string;
        source_bytes: number;
        available_bytes: number;
      } | null>;
      storageProgress: () => Promise<string>;
      moveStorage: (path: string) => Promise<void>;
      exportBackup: () => Promise<string | null>;
      restoreBackup: () => Promise<unknown>;
      autostart: (enabled?: boolean) => Promise<boolean>;
      copy: (text: string) => Promise<void>;
      setTheme: (theme: Appearance["theme"]) => Promise<void>;
    };
  }
}
