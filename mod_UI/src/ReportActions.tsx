import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { sourceNames } from "./types";
import { defaultSettings, type Settings } from "./types";
import { mergeInstallCommands } from "./reportUtils";
import type { SearchResult } from "./utils";
import { getInstallCommand, isErrorResult, isPlainMissingResult, sourceCredibility, uniquePackages } from "./reportUtils";

interface Props {
  results: SearchResult[];
  selectedResults: Set<string>;
  searching: boolean;
  packageCount: number;
  uniqueFoundCount: number;
  searchDuration: number | null;
  onStatusChange: (status: string) => void;
  onRetryMissing: (packages: string[]) => void;
}
const copy = async (content: string, success: string, onStatusChange: Props["onStatusChange"]) => { if (!content.trim()) { onStatusChange("安装命令尚未生成，请稍后重试"); return; } try { await writeText(content); onStatusChange(success); } catch (error) { onStatusChange(`复制失败: ${error instanceof Error ? error.message : String(error)}`); } };
const download = (name: string, content: string, type: string, onStatusChange: Props["onStatusChange"]) => { const url = URL.createObjectURL(new Blob([content], { type })); const anchor = document.createElement("a"); anchor.href = url; anchor.download = name; document.body.appendChild(anchor); anchor.click(); document.body.removeChild(anchor); URL.revokeObjectURL(url); onStatusChange(`已导出 ${name}`); };
const selectionKey = (result: SearchResult) => `${result.package}\u0001${result.requestedVersion}\u0001${result.source}\u0001${result.repository}\u0001${result.realName}`;

export function ReportActions({
  results,
  selectedResults,
  searching,
  packageCount,
  uniqueFoundCount,
  searchDuration,
  onStatusChange,
  onRetryMissing,
  settings = defaultSettings,
}: Props & { settings?: Settings }) {
  const commandsPending = results.some((result) => result.found && !getInstallCommand(result));
  const found = results.filter((result) => result.found); const selected = results.filter((result) => selectedResults.has(selectionKey(result)));
  const exportCsv = () => { const escape = (value: string | undefined) => { const text = value || ""; return /[,"\n]/.test(text) ? `"${text.replace(/"/g, "\"\"")}"` : text; }; const rows = results.map((r) => [r.package, r.requestedVersion, r.latestVersion, sourceNames[r.source] ?? r.source, sourceCredibility(r.source, r.stage), r.repository, r.found ? "已验证" : r.status || "未找到", r.found ? getInstallCommand(r) : ""].map(escape).join(",")); download("r_package_results.csv", `包名,请求版本,实际版本,来源,可信度,仓库,状态,安装命令\r\n${rows.join("\r\n")}`, "text/csv;charset=utf-8", (message) => onStatusChange(`${message}（${results.length} 条结果）`)); };
  const exportDeclaration = (kind: "renv" | "description") => { const content = kind === "renv" ? JSON.stringify({ R: { Version: "4.4.0", Repositories: [{ Name: "CRAN", URL: "https://cloud.r-project.org" }] }, Packages: Object.fromEntries(found.map((r) => [r.package, { Package: r.package, Version: r.latestVersion || "unknown", Source: r.source, Repository: r.repository }])) }, null, 2) : `Package: project\nType: Package\nTitle: Generated dependency declaration\nVersion: 0.0.0.9000\nImports:\n${found.map((r) => `    ${r.package}${r.latestVersion ? ` (== ${r.latestVersion})` : ""}`).join(",\n")}\n`; download(kind === "renv" ? "renv.lock" : "DESCRIPTION", content, "application/json;charset=utf-8", onStatusChange); };
  const exportMarkdown = () => { const now = new Date(); const status = (r: SearchResult) => r.found ? "已验证" : r.status === "timeout" ? "超时" : r.status === "rateLimited" ? "频率限制" : r.status === "error" ? "检索异常" : "未找到"; const lines = [`# R 包检索报告`, ``, `> 生成时间: ${now.toLocaleString("zh-CN")}`, searchDuration != null ? `> 耗时: ${(searchDuration / 1000).toFixed(1)}s` : "", `> 总计: ${results.length} | 已验证: ${found.length}`, ``, `| 包名 | 版本 | 来源 | 可信度 | 仓库 | 状态 |`, `|---|---|---|---|---|---|`, ...results.map((r) => `| ${r.package} | ${r.latestVersion || "-"} | ${sourceNames[r.source] ?? r.source} | ${sourceCredibility(r.source, r.stage)} | ${r.repository || "-"} | ${status(r)} |`)]; download(`r-packages-report-${now.toISOString().slice(0, 10)}.md`, lines.join("\n"), "text/markdown;charset=utf-8", onStatusChange); };
  const openPages = async () => { const pageResults = [...new Map(found.filter((r) => ["cran", "bioc", "github", "r-forge"].includes(r.source)).map((r) => [`${r.source}:${r.package}`, r])).values()]; if (pageResults.length > 5 && !window.confirm(`将要打开 ${pageResults.length} 个浏览器页面，是否继续？`)) return; let opened = 0; for (const result of pageResults) { try { await invoke("open_package_page", { package: result.realName || result.package, source: result.source, repository: result.repository || "" }); opened++; await new Promise((resolve) => window.setTimeout(resolve, 150)); } catch { /* continue opening remaining pages */ } } onStatusChange(`已打开 ${opened} 个来源网页`); };
  const script = () => { const commands = [...new Set(found.map(getInstallCommand))]; const lines = ["# R Package Installation Script", `# Generated: ${new Date().toLocaleString("zh-CN")}`, "", ...commands]; void copy(lines.join("\n"), `已复制完整安装脚本（${commands.length} 个包）`, onStatusChange); };
  const summary = () => { const groups = ["已验证", "未找到", "异常"]; const values = [uniquePackages(found), uniquePackages(results.filter(isPlainMissingResult)), uniquePackages(results.filter(isErrorResult))]; void copy(["R Package Center 检索报告", `时间: ${new Date().toLocaleString("zh-CN")}`, `输入包: ${packageCount}`, `已验证: ${values[0].length}`, `未找到: ${values[1].length}`, `异常: ${values[2].length}`, ...groups.flatMap((group, index) => ["", `${group}包:`, ...values[index].map((name) => `  - ${name}`)])].join("\n"), "已复制检索结果摘要", onStatusChange); };
  return <div style={{ display: "flex", gap: "6px", flexWrap: "wrap" }} onClickCapture={(event) => {
    const text = (event.target as HTMLElement).closest("button")?.textContent || "";
    if (commandsPending && /复制选中|复制全部指令|复制为脚本|导出 CSV/.test(text)) { event.preventDefault(); event.stopPropagation(); onStatusChange("安装命令尚未生成，请稍后重试"); }
  }}>
    {found.length > 0 && <button type="button" className="button ghost compact-btn" disabled={searching || commandsPending} title="合并选中包；未选中时合并全部。无指定版本的 CRAN/Bioconductor 包安装镜像当前版本，显式版本及其他来源保留原指令。" onClick={() => {
      try { void copy(mergeInstallCommands(selected.length ? selected : found, settings), "已复制合并指令", onStatusChange); }
      catch (error) { onStatusChange(`合并失败: ${String(error)}`); }
    }}>合并指令</button>}
    {results.some(isPlainMissingResult) && <button type="button" className="button ghost compact-btn" onClick={() => { const names = uniquePackages(results.filter(isPlainMissingResult)); void copy(names.join("\n"), `已复制 ${names.length} 个未找到的包名`, onStatusChange); }}>复制未找到</button>}
    {selected.length > 0 && <button type="button" className="button ghost compact-btn" onClick={() => void copy([...new Set(selected.map(getInstallCommand))].join("\n"), `已复制选中安装指令`, onStatusChange)}>复制选中({selectedResults.size})</button>}
    {found.length > 0 && <><button type="button" className="button ghost compact-btn" onClick={() => void copy([...new Set(found.map(getInstallCommand))].join("\n"), `已复制 ${new Set(found.map(getInstallCommand)).size} 条安装指令`, onStatusChange)}>复制全部指令</button><button type="button" className="button ghost compact-btn" onClick={() => void copy([...new Set(found.map((r) => r.package))].join("\n"), "已复制包名", onStatusChange)}>复制包名</button><button type="button" className="button ghost compact-btn" onClick={() => void copy(`c(${[...new Set(found.map((r) => r.package))].map((name) => `"${name}"`).join(", ")})`, "已复制 R 向量", onStatusChange)}>复制 R 向量</button><button type="button" className="button ghost compact-btn" onClick={script}>复制为脚本</button><button type="button" className="button ghost compact-btn" onClick={() => void openPages()}>打开来源网页</button><button type="button" className="button ghost compact-btn" onClick={() => exportDeclaration("renv")}>导出 renv.lock</button><button type="button" className="button ghost compact-btn" onClick={() => exportDeclaration("description")}>导出 DESCRIPTION</button></>}
    {results.some((r) => !r.found) && <button type="button" className="button ghost compact-btn" disabled={searching} onClick={() => onRetryMissing(uniquePackages(results.filter((r) => !r.found)))}>重试全部失败</button>}{results.some(isErrorResult) && <button type="button" className="button ghost compact-btn" disabled={searching} onClick={() => onRetryMissing(uniquePackages(results.filter(isErrorResult)))}>重试异常</button>}
    <button type="button" className="button ghost compact-btn" onClick={exportCsv}>导出 CSV</button><button type="button" className="button ghost compact-btn" onClick={() => void copy(JSON.stringify({ generatedAt: new Date().toISOString(), searchDurationMs: searchDuration, packageCount, uniqueFoundCount, results }, null, 2), `已复制 JSON（${results.length} 条结果）`, onStatusChange)}>复制 JSON</button><button type="button" className="button ghost compact-btn" onClick={exportMarkdown}>导出 MD</button><button type="button" className="button ghost compact-btn" onClick={summary}>复制摘要</button>
  </div>;
}
