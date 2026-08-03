import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  dedupePackageInput, formatError, generateSystemRequirementsScript,
  MAX_HISTORY_RECORDS, MAX_INPUT_CHARS, MAX_INPUT_LINE_BYTES, MAX_PACKAGE_LINES,
  MAX_SCRIPT_CHARS, methodSupportsInput, nonEmptyLineBytesExceeds,
  normalizePackageInputDisplay, scriptValueTooLarge, trimTrailingBlankLines,
  utf8Length, type HistoryRecord,
} from "./utils";
import { type Ecosystem, type InputRules, type Method, type Settings, type View } from "./types";

type SetStatus = (value: string) => void;

interface AppActionContext {
  view: View;
  setView: (view: View) => void;
  input: string;
  setInput: (value: string) => void;
  inputProfile: ReturnType<typeof import("./utils").classifyInputProfile>;
  inputRules: InputRules;
  inputTooLarge: boolean;
  method: Method;
  setMethod: (method: Method) => void;
  ecosystem: Ecosystem;
  pipIndex: string;
  rBinaryMirror: string;
  conditional: boolean;
  setConditional: (value: boolean) => void;
  installDependencies: boolean;
  setInstallDependencies: (value: boolean) => void;
  showRemoteVersion: boolean;
  setShowRemoteVersion: (value: boolean) => void;
  verifyInstall: boolean;
  setVerifyInstall: (value: boolean) => void;
  settings: Settings;
  updateAndPersistSettings: (update: (current: Settings) => Settings) => void;
  searching: boolean;
  searchingRef: React.MutableRefObject<boolean>;
  hasSearchEvidenceRef: React.MutableRefObject<boolean>;
  latestInputRef: React.MutableRefObject<string>;
  latestScriptRef: React.MutableRefObject<string>;
  copyWithLineNumbersRef: React.MutableRefObject<boolean>;
  setLogs: (logs: string[]) => void;
  setStatus: SetStatus;
  requestSeq: React.MutableRefObject<number>;
  setScript: (value: string) => void;
  startSearch: (...args: any[]) => void;
  startBinarySearch: (...args: any[]) => void;
  startMultiEcosystemSearch: (...args: any[]) => void;
  stopSearch: () => void;
  sanitizeHistoryList: (value: unknown) => HistoryRecord[];
  enqueueHistorySave: (build: (current: HistoryRecord[]) => HistoryRecord[]) => Promise<void>;
  copyHistoryRecord: (record: HistoryRecord) => Promise<void>;
  deleteHistoryRecord: (id: string) => Promise<void>;
  clearAllHistory: () => Promise<void>;
  setInputRulesBusy: (busy: boolean) => void;
}

export function useAppActions(context: AppActionContext) {
  const {
    view, setView, input, setInput, inputProfile, inputRules, inputTooLarge, method, setMethod,
    ecosystem, rBinaryMirror,
    conditional, setConditional, installDependencies, setInstallDependencies, showRemoteVersion,
    setShowRemoteVersion, verifyInstall, setVerifyInstall, settings,
    updateAndPersistSettings, searching, searchingRef, hasSearchEvidenceRef, latestInputRef,
    latestScriptRef, copyWithLineNumbersRef, setLogs, setStatus, requestSeq, setScript,
    startSearch, startBinarySearch, startMultiEcosystemSearch, stopSearch,
    sanitizeHistoryList, enqueueHistorySave, setInputRulesBusy,
  } = context;

  function acceptInputValue(value: string, source: "manual" | "clipboard") {
    const displayValue = normalizePackageInputDisplay(value);
    const normalizedValue = source === "clipboard" ? trimTrailingBlankLines(displayValue) : displayValue;
    if (searchingRef.current) { setStatus("检索期间不能修改输入，请先停止当前任务"); return "rejected"; }
    if (normalizedValue.length > MAX_INPUT_CHARS || /[\p{C}]/u.test(normalizedValue.replace(/[\r\n\t]/g, "")) ||
      nonEmptyLineBytesExceeds(normalizedValue, MAX_INPUT_LINE_BYTES) || utf8Length(normalizedValue) > MAX_INPUT_CHARS) {
      setStatus(`${source === "clipboard" ? "剪贴板内容" : "输入"}超出限制或包含非法字符：最多 ${MAX_PACKAGE_LINES} 行、总计 ${MAX_INPUT_CHARS} 字节、单行 ${MAX_INPUT_LINE_BYTES} 字节`);
      return "rejected";
    }
    const clears = normalizedValue !== latestInputRef.current && hasSearchEvidenceRef.current;
    if (clears) { hasSearchEvidenceRef.current = false; setLogs([]); }
    latestInputRef.current = normalizedValue;
    setInput(normalizedValue);
    if (clears && source === "manual") setStatus("输入已变更，检索日志已清除（已验证的来源信息保留）");
    return clears ? "cleared" : "accepted";
  }

  async function pasteInput() {
    try { const value = await readText(); if (value) { const result = acceptInputValue(value, "clipboard"); if (result !== "rejected") setStatus(result === "cleared" ? "已从剪贴板粘贴，检索日志已清除（来源信息保留）" : "已从剪贴板粘贴"); } }
    catch (error) { setStatus(`粘贴失败: ${formatError(error)}`); }
  }

  function handleStartSearch() {
    if (ecosystem === "r-binary") { setView("report"); void startBinarySearch(input, settings, inputTooLarge, rBinaryMirror, () => setView("report")); return; }
    if (ecosystem === "pip" || ecosystem === "conda") { void startMultiEcosystemSearch(input, ecosystem, settings, inputTooLarge, () => setView("report")); return; }
    startSearch(input, settings, inputTooLarge, () => setView("report"), () => setMethod("auto"));
  }

  async function copyScript() {
    const snapshot = latestScriptRef.current;
    if (!snapshot || snapshot === "等待输入...") return;
    if (scriptValueTooLarge(snapshot)) { setStatus(`脚本内容过长，最多允许 ${MAX_SCRIPT_CHARS} 字节`); return; }
    try {
      const records = await invoke<HistoryRecord[]>("build_history_records", { script: snapshot });
      const cleanRecords = sanitizeHistoryList(records);
      const taskRecords = cleanRecords.map((record) => ({ ...record, input, method, conditional, installDependencies, showRemoteVersion, verifyInstall, cranMirror: settings.cranMirror }));
      const text = copyWithLineNumbersRef.current ? snapshot.split("\n").map((line, i) => `${String(i + 1).padStart(3, " ")}  ${line}`).join("\n") : snapshot;
      await writeText(text);
      await enqueueHistorySave((current) => { const commands = new Set(taskRecords.map((record) => record.command)); return [...taskRecords, ...current.filter((record) => !commands.has(record.command))].slice(0, MAX_HISTORY_RECORDS); });
      setStatus(`已复制脚本并记录 ${cleanRecords.length} 条命令`);
    } catch (error) { setStatus(`复制失败: ${formatError(error)}`); }
  }

  function downloadFile(content: string, name: string) {
    const url = URL.createObjectURL(new Blob([content], { type: "text/plain;charset=utf-8" }));
    const a = document.createElement("a"); a.href = url; a.download = name; document.body.appendChild(a); a.click(); document.body.removeChild(a); URL.revokeObjectURL(url);
  }
  function downloadScript() {
    const snapshot = latestScriptRef.current; if (!snapshot || snapshot === "等待输入...") return;
    const name = ecosystem === "r" || ecosystem === "r-binary" ? "install_packages.R" : ecosystem === "pip" ? "install_packages.sh" : "install_conda.sh";
    downloadFile(snapshot, name); setStatus(`已下载 ${ecosystem === "r" || ecosystem === "r-binary" ? "R" : ecosystem === "pip" ? "Pip" : "Conda"} 安装脚本`);
  }
  function downloadWrapperScript(kind: "powershell" | "bash") {
    const snapshot = latestScriptRef.current; if (!snapshot || snapshot === "等待输入..." || scriptValueTooLarge(snapshot)) return;
    const multi = ecosystem === "pip" || ecosystem === "conda";
    const wrapper = multi && kind === "bash" ? snapshot : multi ? `$ErrorActionPreference = "Stop"\n${snapshot.split("\n").filter(Boolean).map((line) => `& ${line}`).join("\n")}\n` : kind === "powershell" ? `# R links package installer\n$ErrorActionPreference = "Stop"\n$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path\n$rscript = Get-Command Rscript -ErrorAction SilentlyContinue\nif (-not $rscript) { Write-Error "Rscript was not found in PATH."; exit 127 }\n& $rscript.Source -f (Join-Path $scriptDir "install_packages.R")\nif ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\nWrite-Host "R package installation completed."\n` : `#!/usr/bin/env bash\nset -u\nSCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"\nRscript "$SCRIPT_DIR/install_packages.R"\n`;
    const name = multi ? kind === "powershell" ? `${ecosystem}_install.ps1` : ecosystem === "pip" ? "install_packages.sh" : "install_conda.sh" : kind === "powershell" ? "install_packages.ps1" : "install_packages.sh";
    downloadFile(wrapper, name); setStatus(`已下载 ${kind === "powershell" ? ".ps1" : ".sh"} 包装脚本，请与 install_packages.R 放在同一目录`);
  }
  function downloadSystemRequirements(kind: "bash" | "powershell") { const name = kind === "bash" ? "setup_sysreqs.sh" : "setup_sysreqs.ps1"; downloadFile(generateSystemRequirementsScript(input, kind), name); setStatus(`已下载系统依赖准备脚本 ${name}`); }
  async function cleanComments() {
    const source = latestScriptRef.current; if (scriptValueTooLarge(source)) { setStatus(`脚本内容过长，最多允许 ${MAX_SCRIPT_CHARS} 字节`); return; }
    const seq = requestSeq.current + 1; requestSeq.current = seq;
    try { const cleaned = await invoke<string>("clean_script", { script: source }); if (seq !== requestSeq.current || source !== latestScriptRef.current) return; setScript(cleaned); setStatus("已移除脚本注释"); }
    catch (error) { if (seq === requestSeq.current && source === latestScriptRef.current) setStatus(`清理失败: ${formatError(error)}`); }
  }
  async function applyHistoryRecord(record: HistoryRecord) {
    const clean = sanitizeHistoryList([record])[0]; if (!clean) return;
    let value = clean.input || clean.packageName; const match = clean.command.match(/install_url\("([^"]+)"/); if (match?.[1]) value = match[1];
    if (acceptInputValue(value, "manual") !== "rejected") { if (clean.input) setMethod(clean.method || "auto"); else if (clean.toolName === "GitHub") setMethod("github"); else if (clean.toolName === "Bioconductor") setMethod("biocManager"); else if (clean.toolName === "remotes") setMethod(clean.command.includes("install_url") ? "remotes" : "auto"); else if (clean.toolName === "devtools") setMethod("devtools"); else if (clean.toolName === "base R") setMethod(clean.command.includes("packageVersion") ? "version" : "base"); if (clean.input) { setConditional(clean.conditional ?? true); setInstallDependencies(clean.installDependencies ?? true); setShowRemoteVersion(clean.showRemoteVersion ?? true); setVerifyInstall(clean.verifyInstall ?? false); if (clean.cranMirror) updateAndPersistSettings((current) => ({ ...current, cranMirror: clean.cranMirror ?? current.cranMirror })); } setView("workspace"); setStatus(`已恢复历史任务 ${clean.packageName} 至工作台`); }
  }
  function handleTempFilter(text: string, mode: "chars" | "lines") { if (!text) return; let next = ""; if (mode === "chars") { try { next = input.replace(new RegExp(text, "g"), ""); } catch { next = input.split(text).join(""); } } else { let regex: RegExp | null = null; try { regex = new RegExp(text); } catch {} next = input.split(/\r?\n/).filter((line) => regex ? !regex.test(line) : !line.includes(text)).join("\n"); } acceptInputValue(next, "manual"); setStatus(`已临时过滤: ${mode === "chars" ? "剔除匹配字符" : "剔除匹配整行"}`); }
  async function saveInputRules(rules: InputRules = inputRules) { setInputRulesBusy(true); try { await invoke("save_input_rules", { rules }); setStatus("过滤规则已保存并立即生效"); } catch (error) { setStatus(`保存过滤规则失败: ${formatError(error)}`); } finally { setInputRulesBusy(false); } }
  function isMethodDisabled(candidate: Method) { return !methodSupportsInput(candidate, inputProfile); }

  useEffect(() => { if (inputProfile.total === 0 || methodSupportsInput(method, inputProfile)) return; setMethod(inputProfile.archiveUrls === inputProfile.total ? "remotes" : inputProfile.repositories === inputProfile.total ? "github" : "auto"); }, [inputProfile, method]);
  useEffect(() => { if (view !== "workspace") return; const onKeydown = (e: KeyboardEvent) => { if (!(e.ctrlKey || e.metaKey)) return; if (e.key === "Enter") { e.preventDefault(); if (searching) stopSearch(); else if (input.trim() && !inputTooLarge) handleStartSearch(); } else if (e.shiftKey && e.key.toLowerCase() === "c") { e.preventDefault(); void copyScript(); } else if (!e.shiftKey && e.key.toLowerCase() === "s") { e.preventDefault(); downloadScript(); } else if (e.shiftKey && e.key.toLowerCase() === "k") { e.preventDefault(); if (!searching && input.trim()) acceptInputValue("", "manual"); } else if (!e.shiftKey && e.key.toLowerCase() === "d") { e.preventDefault(); if (!searching && input.trim()) acceptInputValue(dedupePackageInput(input), "manual"); } }; window.addEventListener("keydown", onKeydown); return () => window.removeEventListener("keydown", onKeydown); }, [view, searching, input, inputTooLarge]);
  return { acceptInputValue, pasteInput, handleStartSearch, copyScript, downloadScript, downloadWrapperScript, downloadSystemRequirements, cleanComments, applyHistoryRecord, handleTempFilter, saveInputRules, isMethodDisabled };
}
