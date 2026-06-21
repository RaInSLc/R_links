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
