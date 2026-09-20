import { sourceNames, type Method } from "./types";
import {
  MAX_HISTORY_FIELD_CHARS,
  MAX_INPUT_CHARS,
  MAX_RESULT_FIELD_CHARS,
  MAX_SEARCH_LOGS,
  MAX_SEARCH_RESULT_SCAN,
  MAX_SEARCH_RESULTS,
  MAX_SOURCE_CHARS,
  MAX_STATUS_CHARS,
  MAX_VERSION_CHARS,
  type DependencyDiagnostic,
  type DependencyGraph,
  type HistoryRecord,
  type PublicSettings,
  type SearchResponse,
  type SearchResult,
} from "./utils-types";

const VALID_METHODS: Method[] = [
  "auto",
  "devtools",
  "remotes",
  "github",
  "base",
  "version",
  "biocManager",
  "checkSystem",
];
const DEFAULT_PINNED_METHODS: Method[] = ["auto", "base", "biocManager", "github"];
const SEARCH_STAGE_TIMING_LIMIT = 10;
const SEARCH_STAGE_DURATION_LIMIT_MS = 86_400_000;
const STAGE_TIMING_KEY = "stage";
const CONTROL_CHAR_RE = /[\p{C}]/gu;
const encoder = new TextEncoder();

export const utf8Length = (value: string) => encoder.encode(value).length;

export function truncateUtf8Bytes(value: string, limit: number) {
  if (utf8Length(value) <= limit) return value;
  let bytes = 0;
  let out = "";
  for (const c of value) {
    const n = utf8Length(c);
    if (bytes + n > limit) break;
    bytes += n;
    out += c;
  }
  return out;
}

function stripControl(value: string): string {
  return value.replace(CONTROL_CHAR_RE, "");
}

export function safeStatusText(value: unknown) {
  const text = stripControl(String(value ?? "").trim());
  const truncated = truncateUtf8Bytes(text, MAX_STATUS_CHARS);
  return truncated || "未知错误";
}

export function safeText(value: unknown, limit: number) {
  return truncateUtf8Bytes(stripControl(String(value ?? "").trim()), limit);
}

export const safeBoolean = (value: unknown) => value === true;

export const safeRunId = (value: unknown) =>
  typeof value === "number" && Number.isSafeInteger(value) && value > 0 ? value : 0;

export function safeSource(value: unknown) {
  const source = safeText(value, MAX_SOURCE_CHARS);
  return Object.prototype.hasOwnProperty.call(sourceNames, source) ? source : "none";
}

const STATUS_VALUES = ["found", "notFound", "timeout", "rateLimited", "error"];
const STAGE_VALUES = ["queued", "cacheHit", "searching", "retrying", "final"];

export function safeStatus(value: unknown) {
  const raw = typeof value === "string" ? value : "";
  return STATUS_VALUES.includes(raw) ? raw : "notFound";
}

export function safeStage(value: unknown) {
  const raw = typeof value === "string" ? value : "";
  return STAGE_VALUES.includes(raw) ? raw : "final";
}

export const sanitizeStatus = safeStatus;
export const sanitizeSearchStage = safeStage;

export function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

export function asArray<T>(value: T[] | unknown): T[] {
  return Array.isArray(value) ? value : [];
}

export function mapBounded<T, U>(
  items: readonly T[],
  limit: number,
  mapper: (item: T) => U,
): U[] {
  const out: U[] = [];
  const max = Math.max(0, Math.floor(limit));
  for (let i = 0; i < items.length && i < max; i++) out.push(mapper(items[i]));
  return out;
}

export function sanitizeSearchResult(value: unknown): SearchResult {
  const r = asRecord(value);
  return {
    package: safeText(r.package, MAX_RESULT_FIELD_CHARS),
    requestedVersion: safeText(r.requestedVersion, MAX_VERSION_CHARS),
    latestVersion: safeText(r.latestVersion, MAX_VERSION_CHARS),
    repository: safeText(r.repository, MAX_RESULT_FIELD_CHARS),
    realName: safeText(r.realName, MAX_RESULT_FIELD_CHARS),
    source: safeSource(r.source),
    found: safeBoolean(r.found),
    message: safeStatusText(r.message),
    status: safeStatus(r.status),
    stage: safeStage(r.stage),
  };
}

const IDENTITY_KEY_SEPARATOR = "\u0001";

export function resultIdentityKey(r: SearchResult) {
  const caseSensitive = r.source === "github";
  const normalize = (value: string) => (caseSensitive ? value : value.toLocaleLowerCase());
  return [
    normalize(r.package),
    r.requestedVersion,
    r.source,
    normalize(r.repository),
    normalize(r.realName),
  ].join(IDENTITY_KEY_SEPARATOR);
}

export function formatError(error: unknown) {
  try {
    return safeStatusText(error instanceof Error ? error.message : String(error));
  } catch {
    return "未知错误";
  }
}

export const isActiveInputLine = (value: string) =>
  Boolean(value.trim()) && !value.trim().startsWith("#");

export function nonEmptyLineCountExceeds(value: string, limit: number) {
  let count = 0;
  for (const line of value.split(/\r?\n/)) {
    if (isActiveInputLine(line) && ++count > limit) return true;
  }
  return false;
}

export function dedupeBoundedResults(
  items: readonly unknown[],
  limit: number,
  scanLimit: number,
): SearchResult[] {
  const out: SearchResult[] = [];
  const indexes = new Map<string, number>();
  const maxScan = Math.max(0, Math.floor(scanLimit));
  const maxResults = Math.max(0, Math.floor(limit));
  for (let i = 0; i < items.length && i < maxScan; i++) {
    const item = sanitizeSearchResult(items[i]);
    const key = resultIdentityKey(item);
    const index = indexes.get(key);
    if (index !== undefined) {
      out[index] = item;
    } else if (out.length < maxResults) {
      indexes.set(key, out.length);
      out.push(item);
    }
  }
  return out;
}

function sanitizeStageTiming(value: unknown) {
  const t = asRecord(value);
  return {
    stage: safeStatusText(t[STAGE_TIMING_KEY] || "未知阶段"),
    durationMs: Math.max(0, Math.min(SEARCH_STAGE_DURATION_LIMIT_MS, Number(t.durationMs) || 0)),
  };
}

function sanitizeDependencyGraph(value: unknown) {
  const g = asRecord(value);
  return {
    roots: asArray(g.roots).map(String),
    nodes: asArray(g.nodes).map((node) => {
      const n = asRecord(node);
      return {
        package: String(n.package),
        source: String(n.source),
        version: String(n.version),
        depth: Number(n.depth),
        rootPackages: asArray(n.rootPackages).map(String),
        directDependencyCount: Number(n.directDependencyCount),
        heavyDependencyCount: Number(n.heavyDependencyCount),
        status: String(n.status),
      };
    }),
    edges: asArray(g.edges).map((edge) => {
      const e = asRecord(edge);
      return {
        from: String(e.from),
        to: String(e.to),
        relation: String(e.relation),
        strength: String(e.strength),
        depth: Number(e.depth),
      };
    }),
    summary: (() => {
      const s = asRecord(g.summary);
      return {
        totalNodes: Number(s.totalNodes),
        totalEdges: Number(s.totalEdges),
        heavyNodes: Number(s.heavyNodes),
        lightNodes: Number(s.lightNodes),
        sharedNodes: Number(s.sharedNodes),
      };
    })(),
  };
}

export function sanitizeSearchResponse(value: unknown): SearchResponse {
  const r = asRecord(value);
  const out: SearchResponse = {
    runId: safeRunId(r.runId),
    results: dedupeBoundedResults(asArray(r.results), MAX_SEARCH_RESULTS, MAX_SEARCH_RESULT_SCAN),
    logs: mapBounded(asArray(r.logs), MAX_SEARCH_LOGS, safeStatusText),
    stopped: safeBoolean(r.stopped),
    stageTimings: Array.isArray(r.stageTimings)
      ? r.stageTimings.slice(0, SEARCH_STAGE_TIMING_LIMIT).map(sanitizeStageTiming)
      : [],
  };
  if (r.dependencyGraph) {
    out.dependencyGraph = sanitizeDependencyGraph(r.dependencyGraph);
  }
  return out;
}

const HISTORY_ID_MAX_CHARS = 64;
const HISTORY_CREATED_AT_MAX_CHARS = 32;

export function sanitizeHistoryRecord(value: unknown): HistoryRecord {
  const r = asRecord(value);
  return {
    id: safeText(r.id, HISTORY_ID_MAX_CHARS),
    command: safeText(r.command, MAX_HISTORY_FIELD_CHARS),
    packageName: safeText(r.packageName, MAX_RESULT_FIELD_CHARS),
    version: safeText(r.version, MAX_VERSION_CHARS),
    toolName: safeText(r.toolName, MAX_RESULT_FIELD_CHARS),
    createdAt: safeText(r.createdAt, HISTORY_CREATED_AT_MAX_CHARS),
    input: safeText(r.input, MAX_INPUT_CHARS),
    method: VALID_METHODS.includes(r.method as Method) ? (r.method as Method) : "auto",
    conditional: safeBoolean(r.conditional),
    installDependencies: safeBoolean(r.installDependencies),
    showRemoteVersion: safeBoolean(r.showRemoteVersion),
    verifyInstall: safeBoolean(r.verifyInstall),
    cranMirror: safeText(r.cranMirror, MAX_RESULT_FIELD_CHARS),
  };
}

const PUBLIC_SETTINGS_CRAN_FALLBACK = "https://cloud.r-project.org";
const PUBLIC_SETTINGS_PIP_FALLBACK = "https://pypi.org";
const PUBLIC_SETTINGS_CONDA_FALLBACK = ["conda-forge", "bioconda"];
const PUBLIC_SETTINGS_CONDA_MAX = 20;

function clampedInteger(
  value: unknown,
  min: number,
  max: number,
  fallback: number,
): number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= min && value <= max
    ? value
    : fallback;
}

export function sanitizePublicSettings(value: unknown): PublicSettings {
  const s = asRecord(value);
  const pinned = Array.isArray(s.pinnedMethods)
    ? s.pinnedMethods.filter(
        (v, i, a): v is Method =>
          typeof v === "string" && VALID_METHODS.includes(v as Method) && a.indexOf(v) === i,
      )
    : [];
  return {
    proxy: safeText(s.proxy, MAX_RESULT_FIELD_CHARS),
    githubTokenConfigured: safeBoolean(s.githubTokenConfigured),
    cranMirror: safeText(s.cranMirror, MAX_RESULT_FIELD_CHARS) || PUBLIC_SETTINGS_CRAN_FALLBACK,
    rLibPath: safeText(s.rLibPath, MAX_RESULT_FIELD_CHARS),
    fullSearch: safeBoolean(s.fullSearch),
    searchConcurrency: clampedInteger(s.searchConcurrency, 1, 12, 6),
    archiveGithubMajorGap: clampedInteger(s.archiveGithubMajorGap, 0, 10, 1),
    conditional: safeBoolean(s.conditional),
    installDependencies: safeBoolean(s.installDependencies),
    showRemoteVersion: safeBoolean(s.showRemoteVersion),
    useCache: safeBoolean(s.useCache),
    maxCacheEntries: clampedInteger(s.maxCacheEntries, 1, 10_000, 1000),
    useFilter: safeBoolean(s.useFilter),
    resolveDependencies: safeBoolean(s.resolveDependencies),
    maxDependencyDepth: clampedInteger(s.maxDependencyDepth, 1, 5, 2),
    includeLightDependencies: safeBoolean(s.includeLightDependencies),
    maxDependencyNodes: clampedInteger(s.maxDependencyNodes, 1, 500, 100),
    pinnedMethods: pinned.length ? pinned : [...DEFAULT_PINNED_METHODS],
    pipIndex: safeText(s.pipIndex, MAX_RESULT_FIELD_CHARS) || PUBLIC_SETTINGS_PIP_FALLBACK,
    condaChannels: Array.isArray(s.condaChannels)
      ? s.condaChannels
          .filter((v): v is string => typeof v === "string")
          .map((v) => safeText(v, MAX_RESULT_FIELD_CHARS))
          .filter(Boolean)
          .slice(0, PUBLIC_SETTINGS_CONDA_MAX)
      : PUBLIC_SETTINGS_CONDA_FALLBACK,
  };
}

function detectCycle(node: string, path: string[], visiting: Set<string>, settled: Set<string>, cycles: Set<string>, adjacency: Map<string, string[]>, out: DependencyDiagnostic[]) {
  // `settled` 是记忆化：该节点已被完整展开且其可达范围内不再存在未发现的环。
  // 依赖图是多汇聚的有向图，同一节点可能被大量路径重复到达；缺少记忆化时
  // 深链会退化为指数级重复遍历进而卡死渲染。环检测的正确性不受影响：
  // 任何经过该节点的环都会在它第一次被展开时（`visiting` 命中）记录。
  if (settled.has(node)) return;
  if (visiting.has(node)) {
    const start = path.indexOf(node);
    if (start < 0) return;
    const cycle = path.slice(start).concat(node);
    const key = [...cycle].sort().join("|");
    if (!cycles.has(key)) {
      cycles.add(key);
      out.push({ type: "cycle", packages: cycle, detail: `检测到循环依赖：${cycle.join(" -> ")}` });
    }
    return;
  }
  visiting.add(node);
  for (const child of adjacency.get(node) ?? []) {
    detectCycle(child, [...path, node], visiting, settled, cycles, adjacency, out);
  }
  visiting.delete(node);
  settled.add(node);
}

function collectVersionConflicts(graph: DependencyGraph, out: DependencyDiagnostic[]) {
  const versions = new Map<string, Set<string>>();
  for (const node of graph.nodes) {
    if (node.version === "unknown") continue;
    const key = node.package.toLowerCase();
    const set = versions.get(key) ?? new Set<string>();
    set.add(node.version);
    versions.set(key, set);
  }
  for (const [key, set] of versions) {
    if (set.size > 1) {
      out.push({
        type: "version-conflict",
        packages: [key],
        detail: `${key} 存在多个版本：${[...set].join(", ")}`,
      });
    }
  }
}

export function diagnoseDependencyGraph(graph: DependencyGraph): DependencyDiagnostic[] {
  const out: DependencyDiagnostic[] = [];
  const adjacency = new Map<string, string[]>();
  for (const edge of graph.edges) {
    adjacency.set(edge.from, [...(adjacency.get(edge.from) ?? []), edge.to]);
  }
  const visiting = new Set<string>();
  const settled = new Set<string>();
  const cycles = new Set<string>();
  for (const node of graph.nodes) {
    detectCycle(node.package, [], visiting, settled, cycles, adjacency, out);
  }
  collectVersionConflicts(graph, out);
  return out;
}