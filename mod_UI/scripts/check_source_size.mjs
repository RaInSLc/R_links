import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// 脚本位于 mod_UI/scripts/，工程根为 mod_UI/
const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const roots = [path.join(projectRoot, "src"), path.join(projectRoot, "src-tauri/src")];
const extensions = new Set([".ts", ".tsx", ".rs"]);
const violations = [];
// 与 mod_UI/eslint.config.mjs 的 max-len(200) 对齐：单行字符超过此值即视为
// "压行"。空白行与仅含 // 或 /* */ 注释的行不计入。检测使用全部字符
// （包含字符串/模板/URL），比 eslint 更严格，作为兜底防回归。
const MAX_LINE_CHARS = 200;
const COMMENT_PREFIXES = ["//", "/*", "*", "*/"];

// 与 eslint 的 ignores 对齐：测试文件不参与单行长度门禁（仍参与 500 行总规模门禁）。
function isTestFile(target) {
  const name = path.basename(target);
  return /\.test\.(ts|tsx)$/.test(name);
}

function isCommentOnly(trimmed) {
  return COMMENT_PREFIXES.some((prefix) => trimmed.startsWith(prefix));
}

function visit(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) visit(target);
    else if (extensions.has(path.extname(entry.name))) {
      const raw = fs.readFileSync(target, "utf8");
      const lines = raw.split(/\r?\n/);
      const logical = lines.filter((line) => {
        const text = line.trim();
        return text && !isCommentOnly(text);
      }).length;
      const oversizedLines = [];
      const skipLineLen = isTestFile(target);
      if (!skipLineLen) {
        lines.forEach((line, index) => {
          const text = line.trim();
          if (!text || isCommentOnly(text)) {
            return;
          }
          if (line.length > MAX_LINE_CHARS) {
            oversizedLines.push(`${index + 1}行 ${line.length} 字符`);
          }
        });
      }
      if (lines.length > 500 || logical > 500) {
        violations.push(`${target}: ${lines.length} 行，${logical} 逻辑行`);
      }
      if (oversizedLines.length > 0) {
        violations.push(
          `${target}: 存在 ${oversizedLines.length} 行超 ${MAX_LINE_CHARS} 字符（${oversizedLines.slice(0, 3).join("；")}${oversizedLines.length > 3 ? "…" : ""}）`,
        );
      }
    }
  }
}

for (const root of roots) visit(root);
if (violations.length) {
  console.error(`以下源码文件违反规模或单行长度限制：\n${violations.join("\n")}`);
  process.exit(1);
}
console.log("源码文件规模检查通过：所有 TypeScript、TSX 和 Rust 文件均不超过 500 行，单行字符不超过 200。");
