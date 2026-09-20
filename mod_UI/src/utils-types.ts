import type { Method } from "./types";

export interface SearchResult {
  package: string;
  requestedVersion: string;
  latestVersion: string;
  repository: string;
  realName: string;
  source: string;
  found: boolean;
  message: string;
  status?: string;
  stage?: string;
  verifiedCount?: number;
  upVotes?: number;
  downVotes?: number;
  invalidated?: boolean;
}
export interface DependencyNode {
  package: string;
  source: string;
  version: string;
  depth: number;
  rootPackages: string[];
  directDependencyCount: number;
  heavyDependencyCount: number;
  status: string;
}
export interface DependencyEdge { from: string; to: string; relation: string; strength: string; depth: number; }
export interface DependencySummary { totalNodes: number; totalEdges: number; heavyNodes: number; lightNodes: number; sharedNodes: number; }
export interface DependencyGraph { roots: string[]; nodes: DependencyNode[]; edges: DependencyEdge[]; summary: DependencySummary; }
export interface SearchStageTiming { stage: string; durationMs: number; }
export interface SearchResponse { runId: number; results: SearchResult[]; logs: string[]; stopped: boolean; stageTimings?: SearchStageTiming[]; dependencyGraph?: DependencyGraph; }
export interface DependencyDiagnostic { type: "cycle" | "version-conflict"; packages: string[]; detail: string; }
export interface PublicSettings {
  proxy: string;
  githubTokenConfigured: boolean;
  cranMirror: string;
  rLibPath: string;
  fullSearch: boolean;
  searchConcurrency: number;
  archiveGithubMajorGap: number;
  conditional: boolean;
  installDependencies: boolean;
  showRemoteVersion: boolean;
  useCache: boolean;
  maxCacheEntries: number;
  useFilter: boolean;
  resolveDependencies: boolean;
  maxDependencyDepth: number;
  includeLightDependencies: boolean;
  maxDependencyNodes: number;
  pinnedMethods: Method[];
  pipIndex: string;
  condaChannels: string[];
}
export interface HistoryRecord {
  id: string;
  command: string;
  packageName: string;
  version: string;
  toolName: string;
  createdAt: string;
  input?: string;
  method?: Method;
  conditional?: boolean;
  installDependencies?: boolean;
  showRemoteVersion?: boolean;
  verifyInstall?: boolean;
  cranMirror?: string;
}
export interface ReverseDependenciesInfo { package: string; depends: number; imports: number; suggests: number; linkingTo: number; }
export interface MirrorSpeedResult { mirror: string; label: string; latencyMs: number; success: boolean; error?: string; }
export interface SmartSuggestion {
  id: string;
  title: string;
  detail: string;
  actionLabel?: string;
  action?: "method" | "enableVerify" | "replaceInput" | "openSettings" | "enableFullSearch" | "retrySearch";
  method?: string;
  value?: string;
}
export interface NetworkDiagnostic { target: string; url: string; success: boolean; statusCode?: number; latencyMs: number; proxy: string; error?: string; }
export interface ToolchainCheck { tool: string; available: boolean; version: string; advice: string; }
export interface SearchPlanPreview {
  total: number;
  cranLike: number;
  repositories: number;
  archiveUrls: number;
  estimatedRequests: number;
  level: "idle" | "light" | "medium" | "heavy";
  recommendedMode: string;
  summary: string;
  advice: string;
}

export const MAX_PACKAGE_LINES = 500;
export const MAX_SEARCH_RESULTS = MAX_PACKAGE_LINES * 16;
export const MAX_SEARCH_RESULT_SCAN = MAX_SEARCH_RESULTS * 2;
export const MAX_SEARCH_LOGS = 1_000;
export const MAX_STATUS_CHARS = 512;
export const MAX_RESULT_FIELD_CHARS = 2_048;
export const MAX_VERSION_CHARS = 64;
export const MAX_SOURCE_CHARS = 16;
export const MAX_HISTORY_FIELD_CHARS = 8_000;
export const MAX_INPUT_CHARS = 100_000;
export const MAX_INPUT_LINE_BYTES = 2_048;
export const BROWSER_SEARCH_CONFIRM_THRESHOLD = 10;
export const MAX_SEARCH_TABS = 30;
export const MAX_SCRIPT_CHARS = 1_000_000;
export const MAX_TOKEN_CHARS = 512;
export const MAX_HISTORY_RECORDS = 10000;
export const HISTORY_LOAD_WAIT_TIMEOUT_MS = 5_000;
