import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// 脚本位于 mod_UI/scripts/，工程根为 mod_UI/
const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const roots = [path.join(projectRoot, "src"), path.join(projectRoot, "src-tauri/src")];
const extensions = new Set([".ts", ".tsx", ".rs"]);
// 与 mod_UI/eslint.config.mjs 的 max-len(200) 对齐：单行字符超过此值即视为
// "压行"。空白行与仅含 // 或 /* */ 注释的行不计入。检测使用全部字符
// （包含字符串/模板/URL），比 eslint 更严格，作为兜底防回归。
const MAX_LINE_CHARS = 200;
// 单文件总行数与逻辑行数的上限。
const MAX_FILE_LINES = 500;
const COMMENT_PREFIXES = ["//", "/*", "*", "*/"];

/**
 * 历史遗留超长行的「棘轮基线」：mod_UI 相对路径 → 允许的超长行数。
 *
 * 与 eslint.config.mjs 的 max-len 策略对齐（既有源码约 122 行先以 warn 落地，
 * 本次新增模块保持 error）。一次性重构全部历史写法会淹没功能改动，因此改为
 * 棘轮约束：
 * - 未列入基线的文件（新增模块）一行都不允许超长；
 * - 列入基线的文件不得比基线更多，上涨立即失败；
 * - 少于基线时给出下调提示，只减不增。
 * 修完某个文件的超长行后，请把它的数字改小或整条删除。
 */
const MAX_LINE_CHAR_BASELINE = {
  "src-tauri/src/search_multi.rs": 1,
  "src/AppContent.tsx": 12,
  "src/AppPages.tsx": 7,
  "src/CacheSettingsPanel.tsx": 6,
  "src/DependencyViews.tsx": 14,
  "src/InputRulesPanel.tsx": 1,
  "src/ReportActions.tsx": 7,
  "src/ReportLogs.tsx": 1,
  "src/ReportOverview.tsx": 6,
  "src/ReportResultsTable.tsx": 12,
  "src/ReportView.tsx": 6,
  "src/ScriptPreview.tsx": 7,
  "src/SettingsBackupPanel.tsx": 5,
  "src/SettingsNetworkPanel.tsx": 9,
  "src/SettingsStrategyPanel.tsx": 2,
  "src/SettingsView.tsx": 8,
  "src/WorkspaceStrategyPanel.tsx": 3,
  "src/reportUtils.ts": 2,
  "src/useAppActions.ts": 8,
  "src/useScriptGeneration.ts": 1,
  "src/utils-input.ts": 4,
};

// 与 eslint 的 ignores 对齐：测试文件不参与单行长度门禁（仍参与 500 行总规模门禁）。
function isTestFile(target) {
  const name = path.basename(target);
  return /\.test\.(ts|tsx)$/.test(name);
}

function isCommentOnly(trimmed) {
  return COMMENT_PREFIXES.some((prefix) => trimmed.startsWith(prefix));
}

const violations = [];
const ratchetHints = [];

function checkFile(target) {
  const key = path.relative(projectRoot, target).split(path.sep).join("/");
  const lines = fs.readFileSync(target, "utf8").split(/\r?\n/);
  const logical = lines.filter((line) => {
    const text = line.trim();
    return text && !isCommentOnly(text);
  }).length;
  if (lines.length > MAX_FILE_LINES || logical > MAX_FILE_LINES) {
    violations.push(`${key}: ${lines.length} 行，${logical} 逻辑行`);
  }
  if (isTestFile(target)) return;

  const oversized = lines.filter((line) => {
    const text = line.trim();
    return text && !isCommentOnly(text) && line.length > MAX_LINE_CHARS;
  }).length;
  const allowed = MAX_LINE_CHAR_BASELINE[key] ?? 0;
  if (oversized > allowed) {
    const reason = allowed === 0 ? "未列入基线，新增模块必须保持行宽" : `超出基线 ${allowed} 行`;
    violations.push(`${key}: 超 ${MAX_LINE_CHARS} 字符的行 ${oversized} 行（${reason}）`);
  } else if (oversized < allowed) {
    ratchetHints.push(`${key} ${oversized}/${allowed}`);
  }
}

function visit(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) visit(target);
    else if (extensions.has(path.extname(entry.name))) checkFile(target);
  }
}

for (const root of roots) visit(root);

if (violations.length) {
  console.error(`以下源码文件违反规模或单行长度限制：\n${violations.join("\n")}`);
  process.exit(1);
}
if (ratchetHints.length) {
  console.log(`可下调基线的文件（当前/允许）：${ratchetHints.join("、")}`);
}
console.log(
  `源码文件规模检查通过：所有 TypeScript、TSX 和 Rust 文件均不超过 ${MAX_FILE_LINES} 行；`
  + `单行字符不超过 ${MAX_LINE_CHARS}，且未突破历史遗留基线。`,
);
