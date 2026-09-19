export * from "./utils-types";
export * from "./utils-sanitize";
export * from "./utils-input";
export * from "./utils-url";
export * from "./utils-script";
export * from "./utils-suggestions";
import { resultIdentityKey, utf8Length } from "./utils-sanitize";
import type { SearchResult } from "./utils-types";

export function appendBounded<T>(items: T[], item: T, limit: number) { return items.length >= limit ? items : [...items, item]; }
export function upsertBoundedResult(items: SearchResult[], item: SearchResult, limit: number) { const key = resultIdentityKey(item); const index = items.findIndex((current) => resultIdentityKey(current) === key); if (index >= 0) { const next = [...items]; next[index] = item; return next; } return items.length >= limit ? items : [...items, item]; }
export function settingsValueTooLargeOrUnsafe(value: string, limit: number) { return value.length > limit || utf8Length(value) > limit || /[\p{C}]/u.test(value); }
let searchRunCounter = 0;
export function nextSearchRunId() { searchRunCounter = (searchRunCounter + 1) % 1000; return Date.now() * 1000 + searchRunCounter; }
