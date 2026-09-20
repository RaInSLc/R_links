import { MAX_INPUT_CHARS, MAX_INPUT_LINE_BYTES, MAX_PACKAGE_LINES, type SearchPlanPreview } from "./utils-types";
import { utf8Length, isActiveInputLine, isCommentLine, nonEmptyLineCountExceeds } from "./utils-sanitize";
import { classifyUrlInput, isRecognizedUrlInput } from "./utils-url";
import { defaultInputRules, type InputRules } from "./types";
const URL_RE = /^https?:\/\//i;

/**
 * 输入预览口径（计数 / 去重 / 浏览器搜索名单）与后端解析的关系。
 *
 * `InputRules` 的全部可配置项都已在此镜像，且与 Rust `input.rs::parse_inputs_filtered`
 * 的语义逐条对齐：`commentChars`（注释行）、`separators`（分隔符 + 全角标点归一化）、
 * `stripQuotes`（引号剥离）、`stripCParens`（含 R 调用包装前缀表）、`splitSpaces`、
 * `excludeRegex`（行级与段级）、`excludeKeywords`（包名不做大小写区分）。
 *
 * **仍未镜像的部分（后端才是解析权威）**：托管包管理器输入行（`pip install ...` 等）、
 * Markdown 表格行改写、内建黑名单（`if`/`else`/`library` 等词）、以及
 * `parse_input_line` 的包名合法性判定与整批报错语义。因此预览是"与实际解析高度一致"
 * 而非逐字节等价，界面在输入过滤面板中对此有明确提示。
 */

/**
 * 与 Rust `input.rs::strip_r_parens_wrapper` 对齐的包装前缀表。
 * 只剥离前缀与最后一个 `)` 之间的内容，未匹配则原样返回。
 */
const WRAPPER_PREFIXES = [
  "c(",
  "list(",
  "library(",
  "require(",
  "requireNamespace(",
  "install.packages(",
  "devtools::install_github(",
  "remotes::install_github(",
  "remotes::install_version(",
  "BiocManager::install(",
];

const stripParensWrapper = (line: string) => {
  for (const prefix of WRAPPER_PREFIXES) {
    if (line.startsWith(prefix)) {
      const end = line.lastIndexOf(")");
      if (end >= 0) return line.slice(prefix.length, end);
    }
  }
  return line;
};

interface PreviewContext {
  commentChars: string[];
  stripQuotes: boolean;
  stripCParens: boolean;
  splitSpaces: boolean;
  separatorPattern: RegExp | null;
  excludes: RegExp[];
  excludeKeywords: string[];
}

/**
 * 把输入过滤规则预编译成单次遍历可复用的上下文。
 * 逐行调用 `new RegExp` 会在大输入上产生可观测的开销，因此每次顶层调用只编译一次。
 */
/**
 * 防御性取值：规则对象来自后端 `load_input_rules`，缺字段时退回 `defaultInputRules`
 * 的对应默认值，而不是让整个界面因 `undefined is not iterable` 落入错误边界。
 * 显式传入的空数组（例如"不设置任何注释字符"）仍会原样生效。
 */
const listOr = (value: unknown, fallback: string[]): string[] =>
  Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : fallback;
const boolOr = (value: unknown, fallback: boolean): boolean =>
  typeof value === "boolean" ? value : fallback;

const buildPreviewContext = (rules: InputRules): PreviewContext => {
  const safeSeparators = listOr(rules.separators, defaultInputRules.separators)
    .filter(Boolean)
    .sort((a, b) => b.length - a.length);
  const separatorPattern = safeSeparators.length
    ? new RegExp(safeSeparators.map((s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|"))
    : null;
  const excludes: RegExp[] = [];
  for (const pattern of listOr(rules.excludeRegex, [])) {
    try { excludes.push(new RegExp(pattern)); } catch { /* 非法正则与 Rust 侧一致地忽略 */ }
  }
  return {
    commentChars: listOr(rules.commentChars, defaultInputRules.commentChars),
    stripQuotes: boolOr(rules.stripQuotes, defaultInputRules.stripQuotes),
    stripCParens: boolOr(rules.stripCParens, defaultInputRules.stripCParens),
    splitSpaces: boolOr(rules.splitSpaces, defaultInputRules.splitSpaces),
    separatorPattern,
    excludes,
    excludeKeywords: listOr(rules.excludeKeywords, [])
      .filter(Boolean)
      .map((keyword) => keyword.toLowerCase()),
  };
};

/** 与 Rust 一致：排除正则同时作用于「整行」与「拆分后的每一段」。 */
const isExcludedLine = (line: string, context: PreviewContext) =>
  context.excludes.some((regex) => regex.test(line));

const isExcludedSegment = (value: string, context: PreviewContext) =>
  context.excludes.some((regex) => regex.test(value)) ||
  context.excludeKeywords.includes(value.toLowerCase());

const splitLine = (line: string, rules: InputRules, context?: PreviewContext) => {
  const ctx = context ?? buildPreviewContext(rules);
  // 全角标点统一归一化为半角；`；` 的处理与 Rust `input.rs::split_by_separators`
  // 保持一致——`，`、`、`、`；` 三者都映射为 `,`，而不是把全角分号映射为 `;`。
  const t = line.replace(/[，、；]/g, ",").trim();
  if (!t || isCommentLine(t, ctx.commentChars)) return [];
  const content = ctx.stripCParens ? stripParensWrapper(t) : t;
  let parts = (ctx.separatorPattern ? content.split(ctx.separatorPattern) : [content]).map((s) => s.trim());
  if (ctx.splitSpaces) parts = parts.flatMap((s) => s.split(/\s+/));
  return parts
    .map((s) => (ctx.stripQuotes ? s.replace(/^["']|["']$/g, "") : s).trim())
    .filter((s) => Boolean(s) && !isExcludedSegment(s, ctx));
};
export const normalizePackageInputDisplay = (value: string) => { let changed = false; const lines = value.split(/\r?\n/).flatMap((line) => { const t = line.trim(); if (!t.startsWith("|") || !t.endsWith("|")) return [line]; changed = true; const cells = t.replace(/^\|/, "").replace(/\|$/, "").split("|").map((c) => c.trim()); const p = cells[0] ?? ""; return !p || cells.every((c) => /^:?-{2,}:?$/.test(c)) || /^(包名|package)$/i.test(p) ? [] : [p]; }); while (changed && lines.length && !lines[lines.length - 1]?.trim()) lines.pop(); return changed ? lines.join("\n") : value; };
export const trimTrailingBlankLines = (value: string) => value.replace(/(?:\r?\n[\t ]*)+$/g, "");
/** 显式「清理输入」：去首尾空白、去空行、统一全角分隔符，保留注释行。仅供显式操作调用，不得放入 onChange。 */
export const cleanPackageInput = (value: string) => {
  const normalized = normalizePackageInputDisplay(value);
  const lines = normalized
    .split(/\r?\n/)
    .map((line) => line.replace(/[，、；]/g, ",").trim())
    .filter(Boolean);
  return trimTrailingBlankLines(lines.join("\n"));
};
export function extractCanonicalInput(value: string) {
  const out: string[] = [];
  const add = (v: string) => {
    const t = v.trim().replace(/^["']|["']$/g, "").trim();
    if (t && !out.some((x) => x.toLowerCase() === t.toLowerCase())) out.push(t);
  };
  for (const line of value.split(/\r?\n/)) {
    const t = line.trim();
    if (!t || t.startsWith("#")) continue;
    const gh = t.match(/\b(?:remotes|devtools)::install_github\s*\(([^)]*)\)/i);
    if (gh) {
      for (const m of gh[1].matchAll(/["']([A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+)["']/g)) add(m[1]);
      continue;
    }
    const call = t.match(/\b(?:install\.packages|BiocManager::install|library|require)\s*\(([^)]*)\)/i);
    if (call) {
      let found = false;
      for (const m of call[1].matchAll(/["']([^"']+)["']/g)) { add(m[1]); found = true; }
      if (!found) {
        const m = call[1].trim().match(/^([A-Za-z][A-Za-z0-9_.]*)$/);
        if (m) add(m[1]);
      }
      continue;
    }
    if (URL_RE.test(t)) { if (isRecognizedUrlInput(t)) add(t); }
  }
  return out.join("\n");
}
export const activeInputLineCount = (value: string, rules: InputRules = defaultInputRules) => {
  const context = buildPreviewContext(rules);
  return value.split(/\r?\n/).reduce((n, l) => {
    const trimmed = l.trim();
    if (!isActiveInputLine(l, context.commentChars) || isExcludedLine(trimmed, context)) return n;
    return n + splitLine(l, rules, context).length;
  }, 0);
};
export const nonEmptyLineBytesExceeds = (value: string, limit: number) => value.split(/\r?\n/).some((l) => l.trim() && utf8Length(l) > limit);
export const inputHasDisallowedControlCharacters = (value: string) => /[\p{C}]/u.test(value.replace(/[\r\n\t]/g, ""));
export const inputValueTooLarge = (value: string, rules: InputRules = defaultInputRules) =>
  value.length > MAX_INPUT_CHARS
  || inputHasDisallowedControlCharacters(value)
  || nonEmptyLineCountExceeds(value, MAX_PACKAGE_LINES, rules.commentChars)
  || nonEmptyLineBytesExceeds(value, MAX_INPUT_LINE_BYTES)
  || utf8Length(value) > MAX_INPUT_CHARS;
export const githubTokenTextAllowed = (value: string) => /^[\x21-\x7E]*$/.test(value);
export function settingsFieldLabel(field: "proxy" | "githubToken" | "cranMirror" | "rLibPath") {
  const labels = {
    proxy: "网络代理",
    githubToken: "GitHub Token",
    cranMirror: "CRAN 镜像",
    rLibPath: "R 库路径",
  };
  return labels[field];
}
export function parseProjectDependencyFile(fileName: string, text: string) {
  const name = fileName.toLowerCase();
  if (name.endsWith("renv.lock")) {
    try {
      const raw = JSON.parse(text) as {
        Packages?: Record<string, { Package?: string; Version?: string }>;
      };
      const p = Object.values(raw.Packages ?? {}).map((i) => {
        const n = typeof i?.Package === "string" ? i.Package.trim() : "";
        const v = typeof i?.Version === "string" ? i.Version.trim() : "";
        return n ? `${n}${v ? ` ${v}` : ""}` : "";
      }).filter(Boolean);
      return p.length ? p.join("\n") : null;
    } catch { return null; }
  }
  if (name.endsWith("description")) {
    const fields = new Set(["imports", "depends", "linkingto"]);
    const p: string[] = [];
    let current = "";
    for (const line of text.split(/\r?\n/)) {
      const f = line.match(/^([A-Za-z][A-Za-z0-9.-]*):\s*(.*)$/);
      if (f) {
        current = f[1].toLowerCase();
        if (fields.has(current)) p.push(...f[2].split(",").map((x) => x.trim()));
        continue;
      }
      if (fields.has(current) && /^\s+/.test(line)) {
        p.push(...line.trim().split(/,\s*/));
      }
    }
    const out = p.map((x) => x.replace(/\s*\([^)]*\)/g, "").trim()).filter(Boolean);
    return out.length ? [...new Set(out)].join("\n") : null;
  }
  if (name.endsWith("requirements.txt")) {
    const p = text.split(/\r?\n/)
      .map((l) => l.replace(/\s+#.*$/, "").trim())
      .filter((l) => l && !l.startsWith("#") && !l.startsWith("-"));
    return p.length ? p.join("\n") : null;
  }
  return null;
}
export function extractSystemRequirements(fileName: string, text: string) { if (!fileName.toLowerCase().endsWith("description")) return null; const lines = text.split(/\r?\n/), start = lines.findIndex((l) => /^SystemRequirements:\s*/i.test(l)); if (start < 0) return null; const value = [lines[start].replace(/^SystemRequirements:\s*/i, "").trim(), ...lines.slice(start + 1).filter((l) => /^\s+/.test(l)).map((l) => l.trim())].filter(Boolean).join(" ").replace(/\s+/g, " ").trim(); return value || null; }
export function dedupePackageInput(value: string, rules: InputRules = defaultInputRules) {
  const context = buildPreviewContext(rules);
  const seen = new Set<string>();
  const out: string[] = [];
  for (const line of value.split(/\r?\n/)) {
    const t = line.trim();
    if (!t) { if (out.length) out.push(""); continue; }
    if (isCommentLine(t, context.commentChars)) { out.push(line); continue; }
    if (isExcludedLine(t, context)) continue;
    for (const segment of URL_RE.test(t) ? [t] : splitLine(t, rules, context)) {
      if (!seen.has(segment.toLowerCase())) { seen.add(segment.toLowerCase()); out.push(segment); }
    }
  }
  return out.join("\n");
}
export function countDuplicatePackages(value: string, rules: InputRules = defaultInputRules) {
  const context = buildPreviewContext(rules);
  const items = value.split(/\r?\n/)
    .filter((line) => isActiveInputLine(line, context.commentChars) && !isExcludedLine(line.trim(), context))
    .flatMap((line) => URL_RE.test(line.trim())
      ? [line.trim().toLowerCase()]
      : splitLine(line, rules, context).map((s) => s.toLowerCase()));
  return items.length - new Set(items).size;
}
export function classifyInputProfile(value: string, rules: InputRules = defaultInputRules) {
  const context = buildPreviewContext(rules);
  const profile = { total: 0, archiveUrls: 0, repositories: 0 };
  for (const line of value.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || isCommentLine(trimmed, context.commentChars)) continue;
    if (isExcludedLine(trimmed, context)) continue;
    if (URL_RE.test(trimmed)) {
      // 与后端 parse_inputs_filtered 对齐：GitHub 仓库归入 repositories，
      // 归档 URL 归入 archiveUrls，其余 http(s) 行非法且不计入 total。
      const kind = classifyUrlInput(trimmed);
      if (kind === "invalid") continue;
      profile.total++;
      if (kind === "archive") profile.archiveUrls++;
      else profile.repositories++;
    } else {
      for (const segment of splitLine(trimmed, rules, context)) {
        profile.total++;
        if (segment.includes("/")) profile.repositories++;
      }
    }
    if (profile.total > MAX_PACKAGE_LINES) break;
  }
  return profile;
}
export const methodSupportsInput = (method: string, p: { total: number; archiveUrls: number; repositories: number }) => p.total === 0 || method === "auto" || method === "checkSystem" || (["devtools", "remotes"].includes(method) ? p.archiveUrls === p.total : method === "github" ? p.repositories === p.total : p.archiveUrls === 0 && p.repositories === 0);
export function collectBrowserSearchNames(value: string, limit: number, rules: InputRules = defaultInputRules) {
  const context = buildPreviewContext(rules);
  const names: string[] = [];
  const seen = new Set<string>();
  let total = 0;
  for (const line of value.split(/\r?\n/)) {
    if (!isActiveInputLine(line, context.commentChars)) continue;
    if (isExcludedLine(line.trim(), context)) continue;
    for (const segment of splitLine(line, rules, context)) {
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
