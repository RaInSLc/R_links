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

export interface SearchResponse {
  runId: number;
  results: SearchResult[];
  logs: string[];
  stopped: boolean;
}

export interface PublicSettings {
  proxy: string;
  githubTokenConfigured: boolean;
  cranMirror: string;
  fullSearch: boolean;
}

export interface HistoryRecord {
  id: string;
  command: string;
  packageName: string;
  version: string;
  toolName: string;
  createdAt: string;
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

const utf8Encoder = new TextEncoder();

const sourceNames: Record<string, string> = {
  cran: "CRAN",
  bioc: "Bioconductor",
  biocGit: "Bioc 历史版",
  github: "GitHub",
  none: "未找到",
};

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
  return ["found", "notFound", "timeout", "rateLimited"].includes(raw) ? raw : "notFound";
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
  return {
    runId: safeRunId(response.runId),
    results: dedupeBoundedResults(
      asArray(response.results),
      MAX_SEARCH_RESULTS,
      MAX_SEARCH_RESULT_SCAN,
    ),
    logs: mapBounded(asArray(response.logs), MAX_SEARCH_LOGS, safeStatusText),
    stopped: safeBoolean(response.stopped),
  };
}

// -- UI-tier constants --

export const MAX_INPUT_CHARS = 100_000;
export const MAX_INPUT_LINE_BYTES = 2_048;
export const BROWSER_SEARCH_CONFIRM_THRESHOLD = 10;
export const MAX_SEARCH_TABS = 30;
export const MAX_SCRIPT_CHARS = 1_000_000;
export const MAX_TOKEN_CHARS = 512;
export const MAX_HISTORY_RECORDS = 100;
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
      count += 1;
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
  let activeLines = 0;
  for (let index = 0; index < lines.length; index += 1) {
    if (!isActiveInputLine(lines[index])) {
      continue;
    }
    activeLines += 1;
    if (activeLines > MAX_PACKAGE_LINES) {
      break;
    }
    const token = lines[index].trim().split(/\s+/)[0];
    const name = token.split("/").pop() ?? token;
    if (isBrowserSearchPackageName(name) && !seen.has(name)) {
      seen.add(name);
      allNames.push(name);
    }
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
    profile.total += 1;
    if (profile.total > MAX_PACKAGE_LINES) {
      break;
    }
    if (/^https:\/\//i.test(raw)) {
      profile.archiveUrls += 1;
      continue;
    }
    if (raw.split(/\s+/)[0].includes("/")) {
      profile.repositories += 1;
    }
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

let searchRunCounter = 0;
export function nextSearchRunId() {
  searchRunCounter = (searchRunCounter + 1) % 1000;
  return Date.now() * 1000 + searchRunCounter;
}
