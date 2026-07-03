import { sourceNames } from "./types";

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

export interface DependencyEdge {
  from: string;
  to: string;
  relation: string;
  strength: string;
  depth: number;
}

export interface DependencySummary {
  totalNodes: number;
  totalEdges: number;
  heavyNodes: number;
  lightNodes: number;
  sharedNodes: number;
}

export interface DependencyGraph {
  roots: string[];
  nodes: DependencyNode[];
  edges: DependencyEdge[];
  summary: DependencySummary;
}

export interface SearchResponse {
  runId: number;
  results: SearchResult[];
  logs: string[];
  stopped: boolean;
  dependencyGraph?: DependencyGraph;
}

export interface PublicSettings {
  proxy: string;
  githubTokenConfigured: boolean;
  cranMirror: string;
  fullSearch: boolean;
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
}

export interface HistoryRecord {
  id: string;
  command: string;
  packageName: string;
  version: string;
  toolName: string;
  createdAt: string;
}

export interface ReverseDependenciesInfo {
  package: string;
  depends: number;
  imports: number;
  suggests: number;
  linkingTo: number;
}

export interface MirrorSpeedResult {
  mirror: string;
  label: string;
  latencyMs: number;
  success: boolean;
  error?: string;
}

export interface SmartSuggestion {
  id: string;
  title: string;
  detail: string;
  actionLabel?: string;
  action?: "method" | "enableVerify" | "openSettings" | "enableFullSearch" | "retrySearch";
  method?: string;
}

export const MAX_PACKAGE_LINES = 500;
export const MAX_SEARCH_RESULTS = MAX_PACKAGE_LINES * 16;
export const MAX_SEARCH_RESULT_SCAN = MAX_SEARCH_RESULTS * 2;
export const MAX_SEARCH_LOGS = 1_000;
const INPUT_SEPARATORS = /[,;]/;
function splitInputLine(line: string): string[] {
  const trimmed = line.trim();
  if (!trimmed || trimmed.startsWith("#")) return [];
  let content = trimmed;
  const cParens = trimmed.match(/^(?:c|list)\((.+)\)$/s);
  if (cParens) content = cParens[1];
  return content
    .split(INPUT_SEPARATORS)
    .map((s) => s.trim().replace(/^["']|["']$/g, "").trim())
    .filter((s) => s.length > 0);
}
export const MAX_STATUS_CHARS = 512;
export const MAX_RESULT_FIELD_CHARS = 2_048;
export const MAX_VERSION_CHARS = 64;
export const MAX_SOURCE_CHARS = 16;
export const MAX_HISTORY_FIELD_CHARS = 8_000;

const utf8Encoder = new TextEncoder();

export function utf8Length(value: string): number {
  return utf8Encoder.encode(value).length;
}

export function truncateUtf8Bytes(value: string, limit: number): string {
  if (utf8Length(value) <= limit) {
    return value;
  }
  let bytes = 0;
  let output = "";
  for (const character of value) {
    const nextBytes = utf8Length(character);
    if (bytes + nextBytes > limit) {
      break;
    }
    bytes += nextBytes;
    output += character;
  }
  return output;
}

export function safeStatusText(value: unknown): string {
  const text = truncateUtf8Bytes(
    String(value ?? "")
      .trim()
      .replace(/[\p{C}]/gu, ""),
    MAX_STATUS_CHARS,
  );
  return text || "未知错误";
}

export function safeText(value: unknown, limit: number): string {
  return truncateUtf8Bytes(
    String(value ?? "")
      .trim()
      .replace(/[\p{C}]/gu, ""),
    limit,
  );
}

export function safeBoolean(value: unknown): boolean {
  return value === true;
}

export function safeRunId(value: unknown): number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0 ? value : 0;
}

export function safeSource(value: unknown): string {
  const source = safeText(value, MAX_SOURCE_CHARS);
  return Object.prototype.hasOwnProperty.call(sourceNames, source) ? source : "none";
}

export function sanitizeStatus(value: unknown): string {
  const raw = typeof value === "string" ? value : "";
  return ["found", "notFound", "timeout", "rateLimited", "error"].includes(raw) ? raw : "notFound";
}

export function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

export function asArray<T>(value: T[] | unknown): T[] {
  return Array.isArray(value) ? value : [];
}

export function sanitizeSearchResult(value: unknown): SearchResult {
  const result = asRecord(value);
  return {
    package: safeText(result.package, MAX_RESULT_FIELD_CHARS),
    requestedVersion: safeText(result.requestedVersion, MAX_VERSION_CHARS),
    latestVersion: safeText(result.latestVersion, MAX_VERSION_CHARS),
    repository: safeText(result.repository, MAX_RESULT_FIELD_CHARS),
    realName: safeText(result.realName, MAX_RESULT_FIELD_CHARS),
    source: safeSource(result.source),
    found: safeBoolean(result.found),
    message: safeStatusText(result.message),
    status: sanitizeStatus(result.status),
  };
}

export function resultIdentityKey(result: SearchResult): string {
  return [
    result.package.toLocaleLowerCase(),
    result.source,
    result.repository.toLocaleLowerCase(),
    result.realName.toLocaleLowerCase(),
  ].join("\u0001");
}

export function formatError(error: unknown): string {
  try {
    return safeStatusText(error instanceof Error ? error.message : String(error));
  } catch {
    return "未知错误";
  }
}

export function isActiveInputLine(value: string): boolean {
  const trimmed = value.trim();
  return Boolean(trimmed) && !trimmed.startsWith("#");
}

export function nonEmptyLineCountExceeds(value: string, limit: number): boolean {
  let count = 0;
  for (const line of value.split(/\r?\n/)) {
    if (isActiveInputLine(line)) {
      count += 1;
      if (count > limit) {
        return true;
      }
    }
  }
  return false;
}

export function dedupeBoundedResults(
  items: readonly unknown[],
  limit: number,
  scanLimit: number,
): SearchResult[] {
  const results: SearchResult[] = [];
  const indexes = new Map<string, number>();
  const boundedLimit = Math.max(0, Math.floor(limit));
  const boundedScanLimit = Math.max(0, Math.floor(scanLimit));
  for (
    let itemIndex = 0;
    itemIndex < items.length && itemIndex < boundedScanLimit;
    itemIndex += 1
  ) {
    const cleanItem = sanitizeSearchResult(items[itemIndex]);
    const key = resultIdentityKey(cleanItem);
    const index = indexes.get(key);
    if (index !== undefined) {
      results[index] = cleanItem;
    } else if (results.length < boundedLimit) {
      indexes.set(key, results.length);
      results.push(cleanItem);
    }
  }
  return results;
}

export function mapBounded<T, U>(
  items: readonly T[],
  limit: number,
  mapper: (item: T) => U,
): U[] {
  const mapped: U[] = [];
  const boundedLimit = Math.max(0, Math.floor(limit));
  for (let index = 0; index < items.length && index < boundedLimit; index += 1) {
    mapped.push(mapper(items[index]));
  }
  return mapped;
}

export function sanitizeSearchResponse(value: unknown): SearchResponse {
  const response = asRecord(value);
  const cleanResponse: SearchResponse = {
    runId: safeRunId(response.runId),
    results: dedupeBoundedResults(
      asArray(response.results),
      MAX_SEARCH_RESULTS,
      MAX_SEARCH_RESULT_SCAN,
    ),
    logs: mapBounded(asArray(response.logs), MAX_SEARCH_LOGS, safeStatusText),
    stopped: safeBoolean(response.stopped),
  };

  if (response.dependencyGraph) {
    const graph = asRecord(response.dependencyGraph);
    cleanResponse.dependencyGraph = {
      roots: asArray(graph.roots).map(String),
      nodes: asArray(graph.nodes).map((n) => {
        const node = asRecord(n);
        return {
          package: String(node.package),
          source: String(node.source),
          version: String(node.version),
          depth: Number(node.depth),
          rootPackages: asArray(node.rootPackages).map(String),
          directDependencyCount: Number(node.directDependencyCount),
          heavyDependencyCount: Number(node.heavyDependencyCount),
          status: String(node.status),
        };
      }),
      edges: asArray(graph.edges).map((e) => {
        const edge = asRecord(e);
        return {
          from: String(edge.from),
          to: String(edge.to),
          relation: String(edge.relation),
          strength: String(edge.strength),
          depth: Number(edge.depth),
        };
      }),
      summary: (() => {
        const s = asRecord(graph.summary);
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

  return cleanResponse;
}

// -- UI-tier constants --

export const MAX_INPUT_CHARS = 100_000;
export const MAX_INPUT_LINE_BYTES = 2_048;
export const BROWSER_SEARCH_CONFIRM_THRESHOLD = 10;
export const MAX_SEARCH_TABS = 30;
export const MAX_SCRIPT_CHARS = 1_000_000;
export const MAX_TOKEN_CHARS = 512;
export const MAX_HISTORY_RECORDS = 10000;
export const HISTORY_LOAD_WAIT_TIMEOUT_MS = 5_000;

// -- App.tsx helper functions (extracted) --

export function appendBounded<T>(items: T[], item: T, limit: number) {
  if (items.length >= limit) {
    return items;
  }
  return [...items, item];
}

export function upsertBoundedResult(items: SearchResult[], item: SearchResult, limit: number) {
  const key = resultIdentityKey(item);
  const index = items.findIndex((current) => resultIdentityKey(current) === key);
  if (index >= 0) {
    const next = [...items];
    next[index] = item;
    return next;
  }
  if (items.length >= limit) {
    return items;
  }
  return [...items, item];
}

export function inputValueTooLarge(value: string) {
  return (
    value.length > MAX_INPUT_CHARS ||
    inputHasDisallowedControlCharacters(value) ||
    nonEmptyLineCountExceeds(value, MAX_PACKAGE_LINES) ||
    nonEmptyLineBytesExceeds(value, MAX_INPUT_LINE_BYTES) ||
    utf8Length(value) > MAX_INPUT_CHARS
  );
}

export function scriptValueTooLarge(value: string) {
  return value.length > MAX_SCRIPT_CHARS || utf8Length(value) > MAX_SCRIPT_CHARS;
}

export function settingsValueTooLargeOrUnsafe(value: string, limit: number) {
  return value.length > limit || utf8Length(value) > limit || /[\p{C}]/u.test(value);
}

export function githubTokenTextAllowed(value: string) {
  return /^[\x21-\x7E]*$/.test(value);
}

export function settingsFieldLabel(field: "proxy" | "githubToken" | "cranMirror") {
  switch (field) {
    case "proxy":
      return "网络代理";
    case "githubToken":
      return "GitHub Token";
    case "cranMirror":
      return "CRAN 镜像";
  }
}

export function activeInputLineCount(value: string) {
  let count = 0;
  for (const line of value.split(/\r?\n/)) {
    if (isActiveInputLine(line)) {
      count += splitInputLine(line).length;
    }
  }
  return count;
}

export function nonEmptyLineBytesExceeds(value: string, limit: number) {
  for (const line of value.split(/\r?\n/)) {
    if (line.trim() && utf8Length(line) > limit) {
      return true;
    }
  }
  return false;
}

export function inputHasDisallowedControlCharacters(value: string) {
  return /[\p{C}]/u.test(value.replace(/[\r\n\t]/g, ""));
}

export function sanitizePublicSettings(value: unknown): PublicSettings {
  const s = asRecord(value);
  return {
    proxy: safeText(s.proxy, MAX_RESULT_FIELD_CHARS),
    githubTokenConfigured: safeBoolean(s.githubTokenConfigured),
    cranMirror: safeText(s.cranMirror, MAX_RESULT_FIELD_CHARS) || "https://cloud.r-project.org",
    fullSearch: safeBoolean(s.fullSearch),
    conditional: safeBoolean(s.conditional),
    installDependencies: safeBoolean(s.installDependencies),
    showRemoteVersion: safeBoolean(s.showRemoteVersion),
    useCache: safeBoolean(s.useCache),
    maxCacheEntries: typeof s.maxCacheEntries === "number" && Number.isSafeInteger(s.maxCacheEntries) && s.maxCacheEntries >= 1 && s.maxCacheEntries <= 10000 ? s.maxCacheEntries : 1000,
    useFilter: safeBoolean(s.useFilter),
    resolveDependencies: safeBoolean(s.resolveDependencies),
    maxDependencyDepth: typeof s.maxDependencyDepth === "number" && Number.isSafeInteger(s.maxDependencyDepth) && s.maxDependencyDepth >= 1 && s.maxDependencyDepth <= 5 ? s.maxDependencyDepth : 2,
    includeLightDependencies: safeBoolean(s.includeLightDependencies),
    maxDependencyNodes: typeof s.maxDependencyNodes === "number" && Number.isSafeInteger(s.maxDependencyNodes) && s.maxDependencyNodes >= 1 && s.maxDependencyNodes <= 500 ? s.maxDependencyNodes : 100,
  };
}

export function sanitizeHistoryRecord(value: unknown): HistoryRecord {
  const record = asRecord(value);
  return {
    id: safeText(record.id, 64),
    command: safeText(record.command, MAX_HISTORY_FIELD_CHARS),
    packageName: safeText(record.packageName, MAX_RESULT_FIELD_CHARS),
    version: safeText(record.version, MAX_VERSION_CHARS),
    toolName: safeText(record.toolName, MAX_RESULT_FIELD_CHARS),
    createdAt: safeText(record.createdAt, 32),
  };
}

export function isBrowserSearchPackageName(value: string) {
  return /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(value);
}

export function collectBrowserSearchNames(value: string, limit: number) {
  const allNames: string[] = [];
  const seen = new Set<string>();
  const boundedLimit = Math.max(0, Math.floor(limit));
  const lines = value.split(/\r?\n/);
  let totalPackages = 0;
  for (let index = 0; index < lines.length; index += 1) {
    if (!isActiveInputLine(lines[index])) {
      continue;
    }
    for (const segment of splitInputLine(lines[index])) {
      totalPackages += 1;
      if (totalPackages > MAX_PACKAGE_LINES) break;
      const name = segment.split("/").pop() ?? segment;
      if (isBrowserSearchPackageName(name) && !seen.has(name)) {
        seen.add(name);
        allNames.push(name);
      }
    }
    if (totalPackages > MAX_PACKAGE_LINES) break;
  }
  return {
    names: allNames.slice(0, boundedLimit),
    total: allNames.length,
  };
}

export function classifyInputProfile(value: string): { total: number; archiveUrls: number; repositories: number } {
  const profile = { total: 0, archiveUrls: 0, repositories: 0 };
  const lines = value.split(/\r?\n/);
  for (let index = 0; index < lines.length; index += 1) {
    const raw = lines[index].trim();
    if (!raw || raw.startsWith("#")) {
      continue;
    }
    if (/^https:\/\//i.test(raw)) {
      profile.total += 1;
      profile.archiveUrls += 1;
      if (profile.total > MAX_PACKAGE_LINES) break;
      continue;
    }
    for (const segment of splitInputLine(raw)) {
      profile.total += 1;
      if (profile.total > MAX_PACKAGE_LINES) break;
      if (segment.includes("/")) {
        profile.repositories += 1;
      }
    }
    if (profile.total > MAX_PACKAGE_LINES) break;
  }
  return profile;
}

export function methodSupportsInput(method: string, profile: { total: number; archiveUrls: number; repositories: number }) {
  if (profile.total === 0 || method === "auto" || method === "checkSystem") {
    return true;
  }
  if (method === "devtools" || method === "remotes") {
    return profile.archiveUrls === profile.total;
  }
  if (method === "github") {
    return profile.repositories === profile.total;
  }
  return profile.archiveUrls === 0 && profile.repositories === 0;
}

export function buildInputSmartSuggestions(
  input: string,
  profile: { total: number; archiveUrls: number; repositories: number },
  method: string,
  options: { verifyInstall?: boolean } = {},
): SmartSuggestion[] {
  if (profile.total === 0) return [];
  const suggestions: SmartSuggestion[] = [];
  const activeLines = input.split(/\r?\n/).filter(isActiveInputLine);
  const hasVersionHint = activeLines.some((line) => /\b\d+\.\d+(?:[.\-][0-9A-Za-z]+)*\b/.test(line));
  const hasBiocHint = activeLines.some((line) => /\b(?:BiocManager::install|bioconductor|bioc)\b/i.test(line));
  const hasInstallCall = activeLines.some((line) => /\b(?:install\.packages|BiocManager::install|remotes::install_|devtools::install_|library|require)\s*\(/.test(line));
  const hasLikelyNoise = activeLines.some((line) => /(?:ERROR|Warning|Traceback|安装失败|报错|not available|there is no package)/i.test(line));

  if (profile.archiveUrls === profile.total && method !== "remotes" && method !== "devtools") {
    suggestions.push({
      id: "archive-url",
      title: "检测到归档包 URL",
      detail: "建议使用 remotes 安装 URL，避免按普通 CRAN 包名生成脚本。",
      actionLabel: "切换到 remotes",
      action: "method",
      method: "remotes",
    });
  }
  if (profile.repositories === profile.total && method !== "github") {
    suggestions.push({
      id: "github-repo",
      title: "检测到 GitHub 仓库格式",
      detail: "输入包含 owner/repo，建议直接使用 GitHub 安装方式。",
      actionLabel: "切换到 GitHub",
      action: "method",
      method: "github",
    });
  }
  if (hasBiocHint && method !== "biocManager" && profile.archiveUrls === 0 && profile.repositories === 0) {
    suggestions.push({
      id: "bioc-hint",
      title: "检测到 Bioconductor 线索",
      detail: "输入中包含 Bioconductor 或 BiocManager 语义，建议优先使用 Bioconductor 安装方式。",
      actionLabel: "切换到 Bioconductor",
      action: "method",
      method: "biocManager",
    });
  }
  if (hasVersionHint && method === "auto" && profile.archiveUrls === 0 && profile.repositories === 0) {
    suggestions.push({
      id: "version-hint",
      title: "检测到版本号",
      detail: "脚本会优先生成指定版本安装；检索后可同步确认远程版本。",
    });
  }
  if (hasInstallCall || hasLikelyNoise) {
    suggestions.push({
      id: "mixed-text",
      title: "检测到混合文本",
      detail: "输入中可能包含安装命令、报错或日志，可先用临时过滤剔除无关行。",
    });
  }
  if (profile.total > 20 && !options.verifyInstall) {
    suggestions.push({
      id: "large-batch",
      title: "检测到批量安装任务",
      detail: "建议先检索来源并保留安装后验证，便于发现失败包。",
      actionLabel: "开启安装后验证",
      action: "enableVerify",
    });
  }
  return suggestions.slice(0, 3);
}

export function buildResultSmartSuggestions(
  results: SearchResult[],
  options: { fullSearch?: boolean; searching?: boolean } = {},
): SmartSuggestion[] {
  if (options.searching || results.length === 0) return [];
  const suggestions: SmartSuggestion[] = [];
  const rateLimited = results.some((result) => result.status === "rateLimited");
  const errors = results.filter((result) => result.status === "error" || result.status === "timeout").length;
  const missing = results.filter((result) => !result.found).length;
  const found = results.filter((result) => result.found).length;

  if (rateLimited) {
    suggestions.push({
      id: "github-rate-limit",
      title: "检测到 GitHub 限流",
      detail: "建议在网络设置中配置 GitHub Token，提高检索配额并减少失败。",
      actionLabel: "打开网络设置",
      action: "openSettings",
    });
  }
  if (errors > 0) {
    suggestions.push({
      id: "search-errors",
      title: "检索存在超时或异常",
      detail: "建议先重试当前检索；如果仍失败，再检查代理、镜像或网络设置。",
      actionLabel: "重试检索",
      action: "retrySearch",
    });
  }
  if (missing > 0 && found === 0 && !options.fullSearch) {
    suggestions.push({
      id: "not-found-full-search",
      title: "未找到可用来源",
      detail: "建议启用全量检索，命中 CRAN 或 Bioconductor 后继续查询 GitHub。",
      actionLabel: "启用全量检索",
      action: "enableFullSearch",
    });
  }
  return suggestions.slice(0, 3);
}

let searchRunCounter = 0;
export function nextSearchRunId() {
  searchRunCounter = (searchRunCounter + 1) % 1000;
  return Date.now() * 1000 + searchRunCounter;
}
