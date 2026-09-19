import { MAX_INPUT_CHARS, MAX_INPUT_LINE_BYTES, MAX_PACKAGE_LINES, type SearchPlanPreview } from "./utils-types";
import { utf8Length, isActiveInputLine, nonEmptyLineCountExceeds } from "./utils-sanitize";
import { classifyUrlInput, isRecognizedUrlInput } from "./utils-url";
const DEFAULT_SEPARATORS = [",", ";"];
const URL_RE = /^https?:\/\//i;
const splitLine = (line: string, separators: string[] = DEFAULT_SEPARATORS) => { const t = line.replace(/[，、；]/g, (v) => v === "；" ? ";" : ",").trim(); if (!t || t.startsWith("#")) return []; const match = t.match(/^(?:c|list)\((.+)\)$/s); const content = match ? match[1] : t; const safe = separators.filter(Boolean).sort((a, b) => b.length - a.length); const pattern = safe.length ? new RegExp(safe.map((s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")) : null; return (pattern ? content.split(pattern) : [content]).map((s) => s.trim().replace(/^["']|["']$/g, "").trim()).filter(Boolean); };
export const normalizePackageInputDisplay = (value: string) => { let changed = false; const lines = value.split(/\r?\n/).flatMap((line) => { const t = line.trim(); if (!t.startsWith("|") || !t.endsWith("|")) return [line]; changed = true; const cells = t.replace(/^\|/, "").replace(/\|$/, "").split("|").map((c) => c.trim()); const p = cells[0] ?? ""; return !p || cells.every((c) => /^:?-{2,}:?$/.test(c)) || /^(包名|package)$/i.test(p) ? [] : [p]; }); while (changed && lines.length && !lines[lines.length - 1]?.trim()) lines.pop(); return changed ? lines.join("\n") : value; };
export const trimTrailingBlankLines = (value: string) => value.replace(/(?:\r?\n[\t ]*)+$/g, "");
/** 显式「清理输入」：去首尾空白、去空行、统一全角分隔符，保留注释行。仅供显式操作调用，不得放入 onChange。 */
export const cleanPackageInput = (value: string) => {
  const normalized = normalizePackageInputDisplay(value);
  const lines = normalized
    .split(/\r?\n/)
    .map((line) => line.replace(/[，、；]/g, (separator) => (separator === "；" ? ";" : ",")).trim())
    .filter(Boolean);
  return trimTrailingBlankLines(lines.join("\n"));
};
export function extractCanonicalInput(value: string) { const out: string[] = []; const add = (v: string) => { const t = v.trim().replace(/^["']|["']$/g, "").trim(); if (t && !out.some((x) => x.toLowerCase() === t.toLowerCase())) out.push(t); }; for (const line of value.split(/\r?\n/)) { const t = line.trim(); if (!t || t.startsWith("#")) continue; const gh = t.match(/\b(?:remotes|devtools)::install_github\s*\(([^)]*)\)/i); if (gh) { for (const m of gh[1].matchAll(/["']([A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+)["']/g)) add(m[1]); continue; } const call = t.match(/\b(?:install\.packages|BiocManager::install|library|require)\s*\(([^)]*)\)/i); if (call) { let found = false; for (const m of call[1].matchAll(/["']([^"']+)["']/g)) { add(m[1]); found = true; } if (!found) { const m = call[1].trim().match(/^([A-Za-z][A-Za-z0-9_.]*)$/); if (m) add(m[1]); } continue; } if (URL_RE.test(t)) { if (isRecognizedUrlInput(t)) add(t); } } return out.join("\n"); }
export const activeInputLineCount = (value: string, separators?: string[]) => value.split(/\r?\n/).reduce((n, l) => n + (isActiveInputLine(l) ? splitLine(l, separators).length : 0), 0);
export const nonEmptyLineBytesExceeds = (value: string, limit: number) => value.split(/\r?\n/).some((l) => l.trim() && utf8Length(l) > limit);
export const inputHasDisallowedControlCharacters = (value: string) => /[\p{C}]/u.test(value.replace(/[\r\n\t]/g, ""));
export const inputValueTooLarge = (value: string) => value.length > MAX_INPUT_CHARS || inputHasDisallowedControlCharacters(value) || nonEmptyLineCountExceeds(value, MAX_PACKAGE_LINES) || nonEmptyLineBytesExceeds(value, MAX_INPUT_LINE_BYTES) || utf8Length(value) > MAX_INPUT_CHARS;
export const githubTokenTextAllowed = (value: string) => /^[\x21-\x7E]*$/.test(value);
export function settingsFieldLabel(field: "proxy" | "githubToken" | "cranMirror" | "rLibPath") { return ({ proxy: "网络代理", githubToken: "GitHub Token", cranMirror: "CRAN 镜像", rLibPath: "R 库路径" })[field]; }
export function parseProjectDependencyFile(fileName: string, text: string) { const name = fileName.toLowerCase(); if (name.endsWith("renv.lock")) { try { const raw = JSON.parse(text) as { Packages?: Record<string, { Package?: string; Version?: string }> }; const p = Object.values(raw.Packages ?? {}).map((i) => { const n = typeof i?.Package === "string" ? i.Package.trim() : "", v = typeof i?.Version === "string" ? i.Version.trim() : ""; return n ? `${n}${v ? ` ${v}` : ""}` : ""; }).filter(Boolean); return p.length ? p.join("\n") : null; } catch { return null; } } if (name.endsWith("description")) { const fields = new Set(["imports", "depends", "linkingto"]), p: string[] = []; let current = ""; for (const line of text.split(/\r?\n/)) { const f = line.match(/^([A-Za-z][A-Za-z0-9.-]*):\s*(.*)$/); if (f) { current = f[1].toLowerCase(); if (fields.has(current)) p.push(...f[2].split(",").map((x) => x.trim())); continue; } if (fields.has(current) && /^\s+/.test(line)) p.push(...line.trim().split(/,\s*/)); } const out = p.map((x) => x.replace(/\s*\([^)]*\)/g, "").trim()).filter(Boolean); return out.length ? [...new Set(out)].join("\n") : null; } if (name.endsWith("requirements.txt")) { const p = text.split(/\r?\n/).map((l) => l.replace(/\s+#.*$/, "").trim()).filter((l) => l && !l.startsWith("#") && !l.startsWith("-")); return p.length ? p.join("\n") : null; } return null; }
export function extractSystemRequirements(fileName: string, text: string) { if (!fileName.toLowerCase().endsWith("description")) return null; const lines = text.split(/\r?\n/), start = lines.findIndex((l) => /^SystemRequirements:\s*/i.test(l)); if (start < 0) return null; const value = [lines[start].replace(/^SystemRequirements:\s*/i, "").trim(), ...lines.slice(start + 1).filter((l) => /^\s+/.test(l)).map((l) => l.trim())].filter(Boolean).join(" ").replace(/\s+/g, " ").trim(); return value || null; }
export function dedupePackageInput(value: string) { const seen = new Set<string>(), out: string[] = []; for (const line of value.split(/\r?\n/)) { const t = line.trim(); if (!t) { if (out.length) out.push(""); continue; } if (t.startsWith("#")) { out.push(line); continue; } for (const s of URL_RE.test(t) ? [t] : splitLine(t)) { if (!seen.has(s.toLowerCase())) { seen.add(s.toLowerCase()); out.push(s); } } } return out.join("\n"); }
export function countDuplicatePackages(value: string) { const items = value.split(/\r?\n/).filter(isActiveInputLine).flatMap((l) => URL_RE.test(l.trim()) ? [l.trim().toLowerCase()] : splitLine(l).map((s) => s.toLowerCase())); return items.length - new Set(items).size; }
export function classifyInputProfile(value: string, separators?: string[]) {
  const profile = { total: 0, archiveUrls: 0, repositories: 0 };
  for (const line of value.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    if (URL_RE.test(trimmed)) {
      // 与后端 parse_inputs_filtered 对齐：GitHub 仓库归入 repositories，
      // 归档 URL 归入 archiveUrls，其余 http(s) 行非法且不计入 total。
      const kind = classifyUrlInput(trimmed);
      if (kind === "invalid") continue;
      profile.total++;
      if (kind === "archive") profile.archiveUrls++;
      else profile.repositories++;
    } else {
      for (const segment of splitLine(trimmed, separators)) {
        profile.total++;
        if (segment.includes("/")) profile.repositories++;
      }
    }
    if (profile.total > MAX_PACKAGE_LINES) break;
  }
  return profile;
}
export const methodSupportsInput = (method: string, p: { total: number; archiveUrls: number; repositories: number }) => p.total === 0 || method === "auto" || method === "checkSystem" || (["devtools", "remotes"].includes(method) ? p.archiveUrls === p.total : method === "github" ? p.repositories === p.total : p.archiveUrls === 0 && p.repositories === 0);
export function collectBrowserSearchNames(value: string, limit: number, separators?: string[]) {
  const names: string[] = [];
  const seen = new Set<string>();
  let total = 0;
  for (const line of value.split(/\r?\n/)) {
    if (!isActiveInputLine(line)) continue;
    for (const segment of splitLine(line, separators)) {
      if (++total > MAX_PACKAGE_LINES) break;
      const name = segment.split("/").pop() ?? segment;
      // R 包名必须以 ASCII 字母开头，与后端 is_valid_package_name 保持一致。
      if (/^[A-Za-z][A-Za-z0-9._-]{0,127}$/.test(name) && !seen.has(name)) {
        seen.add(name);
        names.push(name);
      }
    }
    if (total > MAX_PACKAGE_LINES) break;
  }
  return { names: names.slice(0, Math.max(0, Math.floor(limit))), total: names.length };
}
export function buildSearchPlanPreview(profile: { total: number; archiveUrls: number; repositories: number }, options: { fullSearch?: boolean; useCache?: boolean; duplicateCount?: number } = {}): SearchPlanPreview { const total = Math.max(0, profile.total), archiveUrls = Math.max(0, Math.min(profile.archiveUrls, total)), repositories = Math.max(0, Math.min(profile.repositories, total - archiveUrls)), cranLike = Math.max(0, total - archiveUrls - repositories), base = cranLike * (options.fullSearch ? 4 : 2) + repositories * 2 + archiveUrls, estimatedRequests = options.useCache ? Math.ceil(base * 0.6) : base, level = total === 0 ? "idle" : estimatedRequests >= 160 || total >= 80 ? "heavy" : estimatedRequests >= 60 || total >= 30 ? "medium" : "light"; const sourceParts = [cranLike ? `CRAN/Bioc ${cranLike}` : "", repositories ? `GitHub ${repositories}` : "", archiveUrls ? `URL ${archiveUrls}` : ""].filter(Boolean); return { total, cranLike, repositories, archiveUrls, estimatedRequests, level, recommendedMode: repositories === total && total > 0 ? "GitHub 优先" : archiveUrls === total && total > 0 ? "URL 安装" : options.fullSearch ? "全量检索" : "快速检索", summary: total === 0 ? "等待输入包名" : `${total} 个输入 · ${sourceParts.join(" · ") || "待识别来源"} · 预计 ${estimatedRequests} 次请求`, advice: total === 0 ? "输入包名后将生成搜索计划。" : level === "heavy" ? "建议开启缓存、配置 GitHub Token，并优先按失败分类重试。" : level === "medium" ? "建议保持缓存开启；如出现限流，可先重试超时或错误分组。" : options.duplicateCount ? "检测到重复输入，建议先去重再开始检索。" : "当前规模适合直接检索。" }; }
