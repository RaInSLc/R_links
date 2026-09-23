import { MAX_RESULT_FIELD_CHARS, MAX_TOKEN_CHARS } from "./utils";
import { methods, type InputRules, type Settings } from "./types";

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

function stringValue(value: unknown, fallback: string, maxLength = MAX_RESULT_FIELD_CHARS) {
  return typeof value === "string" && value.length <= maxLength ? value : fallback;
}

function booleanValue(value: unknown, fallback: boolean) {
  return typeof value === "boolean" ? value : fallback;
}

function numberValue(value: unknown, fallback: number, min: number, max: number) {
  return typeof value === "number" && Number.isFinite(value) ? Math.min(max, Math.max(min, Math.floor(value))) : fallback;
}

function stringList(value: unknown, fallback: string[], maxLength: number, maxItems: number) {
  if (!Array.isArray(value)) return fallback;
  const items = value
    .filter((item): item is string => typeof item === "string")
    .map((item) => item.trim())
    .filter((item, index, list) => item.length > 0 && item.length <= maxLength && list.indexOf(item) === index)
    .slice(0, maxItems);
  return items.length > 0 ? items : fallback;
}

export function sanitizeImportedSettings(value: unknown, current: Settings): Settings {
  const raw = asRecord(value);
  const pinnedMethods = Array.isArray(raw.pinnedMethods)
    ? raw.pinnedMethods
      .filter(
        (item, index, list): item is Settings["pinnedMethods"][number] =>
          typeof item === "string" &&
          methods.some((method) => method.id === item) &&
          list.indexOf(item) === index,
      )
    : current.pinnedMethods;
  return {
    ...current,
    proxy: stringValue(raw.proxy, current.proxy),
    githubToken: stringValue(raw.githubToken, current.githubToken, MAX_TOKEN_CHARS),
    cranMirror: stringValue(raw.cranMirror, current.cranMirror),
    rLibPath: stringValue(raw.rLibPath, current.rLibPath),
    fullSearch: booleanValue(raw.fullSearch, current.fullSearch),
    searchConcurrency: numberValue(raw.searchConcurrency, current.searchConcurrency, 1, 12),
    archiveGithubMajorGap: numberValue(raw.archiveGithubMajorGap, current.archiveGithubMajorGap, 0, 10),
    conditional: booleanValue(raw.conditional, current.conditional),
    installDependencies: booleanValue(raw.installDependencies, current.installDependencies),
    showRemoteVersion: booleanValue(raw.showRemoteVersion, current.showRemoteVersion),
    useCache: booleanValue(raw.useCache, current.useCache),
    maxCacheEntries: numberValue(raw.maxCacheEntries, current.maxCacheEntries, 1, 10000),
    useFilter: booleanValue(raw.useFilter, current.useFilter),
    resolveDependencies: booleanValue(raw.resolveDependencies, current.resolveDependencies),
    maxDependencyDepth: numberValue(raw.maxDependencyDepth, current.maxDependencyDepth, 1, 5),
    includeLightDependencies: booleanValue(raw.includeLightDependencies, current.includeLightDependencies),
    maxDependencyNodes: numberValue(raw.maxDependencyNodes, current.maxDependencyNodes, 1, 500),
    pinnedMethods: pinnedMethods.length > 0 ? pinnedMethods : current.pinnedMethods,
  };
}

export function sanitizeImportedInputRules(value: unknown, current: InputRules): InputRules {
  const raw = asRecord(value);
  const excludeRegex = stringList(raw.excludeRegex, current.excludeRegex, 256, 10).filter(
    (item) => {
      try {
        new RegExp(item);
        return true;
      } catch {
        return false;
      }
    },
  );
  return {
    separators: stringList(raw.separators, current.separators, 16, 20),
    stripQuotes: booleanValue(raw.stripQuotes, current.stripQuotes),
    stripCParens: booleanValue(raw.stripCParens, current.stripCParens),
    commentChars: stringList(raw.commentChars, current.commentChars, 16, 20),
    splitSpaces: booleanValue(raw.splitSpaces, current.splitSpaces),
    excludeRegex,
    excludeKeywords: stringList(raw.excludeKeywords, current.excludeKeywords, 64, 50),
  };
}
