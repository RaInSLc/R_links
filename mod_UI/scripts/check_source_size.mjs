import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// 脚本位于 mod_UI/scripts/，工程根为 mod_UI/
const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const roots = [path.join(projectRoot, "src"), path.join(projectRoot, "src-tauri/src")];
const extensions = new Set([".ts", ".tsx", ".rs"]);
const violations = [];

function visit(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) visit(target);
    else if (extensions.has(path.extname(entry.name))) {
      const lines = fs.readFileSync(target, "utf8").split(/\r?\n/);
      const logical = lines.filter((line) => {
        const text = line.trim();
        return text && !text.startsWith("//") && !text.startsWith("/*") && !text.startsWith("*") && !text.startsWith("*/");
      }).length;
      if (lines.length > 500 || logical > 500) violations.push(`${target}: ${lines.length} 行，${logical} 逻辑行`);
    }
  }
}

for (const root of roots) visit(root);
if (violations.length) {
  console.error(`以下源码文件超过 500 行限制：\n${violations.join("\n")}`);
  process.exit(1);
}
console.log("源码文件规模检查通过：所有 TypeScript、TSX 和 Rust 文件均不超过 500 行。");
