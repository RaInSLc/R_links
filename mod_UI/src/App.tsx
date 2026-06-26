import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import "./App.css";
import { NavButton } from "./components";
import { WorkspaceView } from "./WorkspaceView";
import { ReportView } from "./ReportView";
import { HistoryView } from "./HistoryView";
import { SettingsView } from "./SettingsView";
import { useSettings } from "./useSettings";
import { useHistory } from "./useHistory";
import { useSearch } from "./useSearch";
import {
  formatError, scriptValueTooLarge, activeInputLineCount,
  nonEmptyLineBytesExceeds, methodSupportsInput, classifyInputProfile,
  MAX_INPUT_CHARS, MAX_INPUT_LINE_BYTES, MAX_PACKAGE_LINES,
  MAX_SCRIPT_CHARS, MAX_HISTORY_RECORDS, utf8Length,
  type HistoryRecord,
} from "./utils";
import { type View, type Method, type InputRules, defaultInputRules, defaultSettings } from "./types";

function App() {
  const [view, setView] = useState<View>("workspace");
  const [currentTheme, setCurrentTheme] = useState(() => localStorage.getItem("theme") || "office");
  const [currentFont, setCurrentFont] = useState(() => localStorage.getItem("fontFamily") || "modern");
  const [input, setInput] = useState("");
  const [method, setMethod] = useState<Method>("auto");
  const [conditional, setConditional] = useState(true);
  const [installDependencies, setInstallDependencies] = useState(true);
  const [showRemoteVersion, setShowRemoteVersion] = useState(true);
  const [script, setScriptState] = useState("等待输入...");
  const [status, setStatus] = useState("就绪");
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateMessage, setUpdateMessage] = useState("");
  const [inputRules, setInputRules] = useState<InputRules>(defaultInputRules);
  const [inputRulesBusy, setInputRulesBusy] = useState(false);

  const latestInputRef = useRef("");
  const latestScriptRef = useRef("等待输入...");
  const scriptRequestSeq = useRef(0);

  const search = useSearch(setStatus);
  const settingsHook = useSettings(setStatus);
  const historyHook = useHistory(setStatus);

  const { results, setResults, logs, setLogs, searching, openingSearchTabs,
    searchingRef, hasSearchEvidenceRef,
    startSearch, stopSearch, openSearchTabs } = search;
  const { settings, showToken, setShowToken,
    tokenConfigured, settingsBusy, updateSettingsFromUser,
    acceptSettingValue, persistSettings, clearSavedToken } = settingsHook;
  const { history, historySearch, setHistorySearch,
    sanitizeHistoryList, enqueueHistorySave,
    copyHistoryRecord, deleteHistoryRecord } = historyHook;

  function setScript(next: string) {
    latestScriptRef.current = next;
    setScriptState(next);
  }

  const packageCount = useMemo(() => activeInputLineCount(input), [input]);
  const inputProfile = useMemo(() => classifyInputProfile(input), [input]);
  const inputBytes = useMemo(() => utf8Length(input), [input]);
  const inputTooLarge =
    inputBytes > MAX_INPUT_CHARS ||
    packageCount > MAX_PACKAGE_LINES ||
    nonEmptyLineBytesExceeds(input, MAX_INPUT_LINE_BYTES);
  const scriptTooLarge = useMemo(() => scriptValueTooLarge(script), [script]);
  const foundCount = results.filter((r) => r.found).length;
  const uniqueFoundCount = new Set(results.filter((r) => r.found).map((r) => r.package)).size;
  const summaryProgress = packageCount ? Math.min(100, (uniqueFoundCount / packageCount) * 100) : 0;

  const settingsLoadedRef = useRef(false);
  useEffect(() => {
    if (settingsLoadedRef.current) return;
    if (settings.conditional !== defaultSettings.conditional ||
        settings.installDependencies !== defaultSettings.installDependencies ||
        settings.showRemoteVersion !== defaultSettings.showRemoteVersion) {
      setConditional(settings.conditional);
      setInstallDependencies(settings.installDependencies);
      setShowRemoteVersion(settings.showRemoteVersion);
      settingsLoadedRef.current = true;
    }
  }, [settings.conditional, settings.installDependencies, settings.showRemoteVersion]);

  useEffect(() => {
    invoke<InputRules>("load_input_rules")
      .then((rules) => setInputRules(rules))
      .catch(() => {});
  }, []);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", currentTheme);
  }, [currentTheme]);
  useEffect(() => {
    document.documentElement.setAttribute("data-font", currentFont);
  }, [currentFont]);

  const handleThemeChange = (theme: string) => {
    setCurrentTheme(theme);
    localStorage.setItem("theme", theme);
  };
  const handleFontChange = (font: string) => {
    setCurrentFont(font);
    localStorage.setItem("fontFamily", font);
  };

  function acceptInputValue(value: string, source: "manual" | "clipboard") {
    if (searchingRef.current) {
      setStatus("检索期间不能修改输入，请先停止当前任务");
      return "rejected";
    }
    if (
      value.length > MAX_INPUT_CHARS ||
      /[\p{C}]/u.test(value.replace(/[\r\n\t]/g, "")) ||
      nonEmptyLineBytesExceeds(value, MAX_INPUT_LINE_BYTES) ||
      utf8Length(value) > MAX_INPUT_CHARS
    ) {
      setStatus(
        `${source === "clipboard" ? "剪贴板内容" : "输入"}超出限制或包含非法字符：最多 ${MAX_PACKAGE_LINES} 行、总计 ${MAX_INPUT_CHARS} 字节、单行 ${MAX_INPUT_LINE_BYTES} 字节`,
      );
      return "rejected";
    }
    const clearsSearchEvidence =
      value !== latestInputRef.current && hasSearchEvidenceRef.current;
    if (clearsSearchEvidence) {
      hasSearchEvidenceRef.current = false;
      setResults([]);
      setLogs([]);
    }
    latestInputRef.current = value;
    setInput(value);
    if (clearsSearchEvidence && source === "manual") {
      setStatus("输入已变更，旧检索结果和日志已清除");
    }
    return clearsSearchEvidence ? "cleared" : "accepted";
  }

  async function checkForUpdates() {
    setCheckingUpdate(true);
    setUpdateMessage("正在检查更新...");
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();
      if (update) {
        setUpdateMessage(`发现新版本 ${update.version}，正在下载并安装...`);
        let downloaded = 0;
        let contentLength = 0;
        await update.downloadAndInstall((event) => {
          switch (event.event) {
            case "Started":
              contentLength = event.data?.contentLength || 0;
              setUpdateMessage("正在下载新版本...");
              break;
            case "Progress":
              downloaded += event.data?.chunkLength || 0;
              if (contentLength > 0) {
                setUpdateMessage(`正在下载... ${Math.round((downloaded / contentLength) * 100)}%`);
              }
              break;
            case "Finished":
              setUpdateMessage("下载完成，正在安装...");
              break;
          }
        });
        setUpdateMessage("更新安装成功！请手动关闭并重启应用以生效。");
        window.alert("更新安装成功！请手动关闭并重启应用以生效。");
      } else {
        setUpdateMessage("当前已是最新版本");
      }
    } catch (error) {
      setUpdateMessage(`检查更新失败: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setCheckingUpdate(false);
    }
  }

  async function pasteInput() {
    try {
      const value = await readText();
      if (value) {
        const result = acceptInputValue(value, "clipboard");
        if (result !== "rejected") {
          setStatus(result === "cleared" ? "已从剪贴板粘贴，旧检索结果和日志已清除" : "已从剪贴板粘贴");
        }
      }
    } catch (error) {
      setStatus(`粘贴失败: ${formatError(error)}`);
    }
  }

  useEffect(() => {
    let active = true;
    const timer = window.setTimeout(() => {
      const seq = scriptRequestSeq.current + 1;
      scriptRequestSeq.current = seq;
      if (inputTooLarge) {
        setScript("输入超出限制，无法生成脚本。");
        return;
      }
      invoke<string>("generate_script", {
        input,
        options: { method, conditional, installDependencies, mirror: settings.cranMirror },
        results,
        showRemoteVersion,
      })
        .then((next) => { if (active && seq === scriptRequestSeq.current) setScript(next); })
        .catch((error) => { if (active && seq === scriptRequestSeq.current) setStatus(`生成失败: ${formatError(error)}`); });
    }, 120);
    return () => { active = false; window.clearTimeout(timer); };
  }, [input, method, conditional, installDependencies, showRemoteVersion, settings.cranMirror, results, inputTooLarge]);

  useEffect(() => {
    if (inputProfile.total === 0 || methodSupportsInput(method, inputProfile)) return;
    if (inputProfile.archiveUrls === inputProfile.total) setMethod("remotes");
    else if (inputProfile.repositories === inputProfile.total) setMethod("github");
    else setMethod("auto");
  }, [inputProfile, method]);

  async function copyScript() {
    const snapshot = latestScriptRef.current;
    if (!snapshot || snapshot === "等待输入...") return;
    if (scriptValueTooLarge(snapshot)) {
      setStatus(`脚本内容过长，最多允许 ${MAX_SCRIPT_CHARS} 字节`);
      return;
    }
    try {
      const records = await invoke<HistoryRecord[]>("build_history_records", { script: snapshot });
      const cleanRecords = sanitizeHistoryList(records);
      await writeText(snapshot);
      await enqueueHistorySave((current) => {
        const commands = new Set(cleanRecords.map((r) => r.command));
        return [...cleanRecords, ...current.filter((r) => !commands.has(r.command))].slice(0, MAX_HISTORY_RECORDS);
      });
      setStatus(`已复制脚本并记录 ${cleanRecords.length} 条命令`);
    } catch (error) {
      setStatus(`复制失败: ${formatError(error)}`);
    }
  }

  async function cleanComments() {
    const source = latestScriptRef.current;
    if (scriptValueTooLarge(source)) {
      setStatus(`脚本内容过长，最多允许 ${MAX_SCRIPT_CHARS} 字节`);
      return;
    }
    const seq = scriptRequestSeq.current + 1;
    scriptRequestSeq.current = seq;
    try {
      const cleaned = await invoke<string>("clean_script", { script: source });
      if (seq !== scriptRequestSeq.current || source !== latestScriptRef.current) return;
      setScript(cleaned);
      setStatus("已移除脚本注释");
    } catch (error) {
      if (seq === scriptRequestSeq.current && source === latestScriptRef.current) {
        setStatus(`清理失败: ${formatError(error)}`);
      }
    }
  }

  async function applyHistoryRecord(record: HistoryRecord) {
    const clean = sanitizeHistoryList([record])[0];
    if (!clean) return;
    let valueToLoad = clean.packageName;
    if (clean.command.includes("install_url(")) {
      const match = clean.command.match(/install_url\("([^"]+)"/);
      if (match && match[1]) valueToLoad = match[1];
    }
    const result = acceptInputValue(valueToLoad, "manual");
    if (result !== "rejected") {
      if (clean.toolName === "GitHub") setMethod("github");
      else if (clean.toolName === "Bioconductor") setMethod("biocManager");
      else if (clean.toolName === "remotes") setMethod(clean.command.includes("install_url") ? "remotes" : "auto");
      else if (clean.toolName === "devtools") setMethod("devtools");
      else if (clean.toolName === "base R") setMethod(clean.command.includes("packageVersion") ? "version" : "base");
      setView("workspace");
      setStatus(`已加载历史命令 ${clean.packageName} 至工作台`);
    }
  }

  function isMethodDisabled(candidate: Method) {
    return !methodSupportsInput(candidate, inputProfile);
  }

  async function saveInputRules() {
    setInputRulesBusy(true);
    try {
      await invoke("save_input_rules", { rules: inputRules });
      setStatus("过滤规则已保存并立即生效");
    } catch (error) {
      setStatus(`保存过滤规则失败: ${formatError(error)}`);
    } finally {
      setInputRulesBusy(false);
    }
  }

  function handleStartSearch() {
    startSearch(input, settings, inputTooLarge, () => setView("report"), () => setMethod("auto"));
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">R</div>
          <div>
            <strong>Package Center</strong>
            <span>R 包命令工作台</span>
          </div>
        </div>
        <nav className="nav-list" aria-label="主导航">
          <NavButton active={view === "workspace"} label="工作台" code="01" onClick={() => setView("workspace")} />
          <NavButton active={view === "report"} label="检索报告" code="02" badge={results.length} onClick={() => setView("report")} />
          <NavButton active={view === "history"} label="命令历史" code="03" badge={history.length} onClick={() => setView("history")} />
          <NavButton active={view === "settings"} label="网络设置" code="04" onClick={() => setView("settings")} />
        </nav>
        <div className="sidebar-summary">
          <span>当前任务</span>
          <strong>{searching ? "检索中" : `${packageCount} 个输入`}</strong>
          <progress className="summary-track" value={summaryProgress} max={100} aria-label="已验证包比例" />
          <small>{results.length ? `${foundCount} 条来源记录` : "等待开始"}</small>
        </div>
      </aside>

      <main className="main-area">
        <header className="topbar">
          <div>
            <span className="eyebrow">R PACKAGE INSTALLATION</span>
            <h1>
              {view === "workspace" && "安装命令工作台"}
              {view === "report" && "多源检索报告"}
              {view === "history" && "命令历史"}
              {view === "settings" && "网络与镜像设置"}
            </h1>
          </div>
          <div className={`status-chip ${searching ? "active" : ""}`} role="status" aria-live="polite" aria-atomic="true">
            <i aria-hidden="true" />
            {status}
          </div>
        </header>

        <section className="content">
          {view === "workspace" && (
            <WorkspaceView
              input={input} inputTooLarge={inputTooLarge} inputProfile={inputProfile}
              method={method} conditional={conditional} installDependencies={installDependencies}
              showRemoteVersion={showRemoteVersion} settings={settings}
              script={script} scriptTooLarge={scriptTooLarge}
              searching={searching} openingSearchTabs={openingSearchTabs}
              onInputChange={acceptInputValue} onPaste={pasteInput}
              onClear={() => acceptInputValue("", "manual")}
              onOpenSearchTabs={() => openSearchTabs(input, inputTooLarge)}
              onStartSearch={handleStartSearch} onStopSearch={stopSearch}
              onMethodChange={setMethod}
              onConditionalChange={setConditional}
              onInstallDependenciesChange={setInstallDependencies}
              onShowRemoteVersionChange={setShowRemoteVersion}
              onFullSearchChange={(v) => updateSettingsFromUser((c) => ({ ...c, fullSearch: v }))}
              onUseCacheChange={(v) => {
                updateSettingsFromUser((c) => ({ ...c, useCache: v }));
                persistSettings({ useCache: v });
              }}
              onUseFilterChange={(v) => {
                updateSettingsFromUser((c) => ({ ...c, useFilter: v }));
                persistSettings({ useFilter: v });
              }}
              onCopyScript={copyScript} onCleanComments={cleanComments}
              isMethodDisabled={isMethodDisabled}
            />
          )}
          {view === "report" && (
            <ReportView
              results={results} logs={logs}
              packageCount={packageCount} uniqueFoundCount={uniqueFoundCount}
              searching={searching} onClearLogs={() => setLogs([])}
            />
          )}
          {view === "history" && (
            <HistoryView
              history={history} historySearch={historySearch}
              onHistorySearchChange={setHistorySearch}
              onApplyRecord={applyHistoryRecord}
              onCopyRecord={copyHistoryRecord}
              onDeleteRecord={deleteHistoryRecord}
            />
          )}
          {view === "settings" && (
            <SettingsView
              settings={settings} tokenConfigured={tokenConfigured}
              showToken={showToken} settingsBusy={settingsBusy}
              currentTheme={currentTheme} currentFont={currentFont}
              checkingUpdate={checkingUpdate} updateMessage={updateMessage}
              onProxyChange={(v) => acceptSettingValue("proxy", v)}
              onTokenChange={(v) => acceptSettingValue("githubToken", v)}
              onTokenToggle={() => setShowToken((v) => !v)}
              onClearToken={clearSavedToken}
              onFullSearchChange={(v) => updateSettingsFromUser((c) => ({ ...c, fullSearch: v }))}
              onUseCacheChange={(v) => updateSettingsFromUser((c) => ({ ...c, useCache: v }))}
              onUseFilterChange={(v) => updateSettingsFromUser((c) => ({ ...c, useFilter: v }))}
              onMaxCacheEntriesChange={(v) => updateSettingsFromUser((c) => ({ ...c, maxCacheEntries: v }))}
              onConditionalChange={(v) => {
                setConditional(v);
                updateSettingsFromUser((c) => ({ ...c, conditional: v }));
                persistSettings({ conditional: v });
              }}
              onInstallDependenciesChange={(v) => {
                setInstallDependencies(v);
                updateSettingsFromUser((c) => ({ ...c, installDependencies: v }));
                persistSettings({ installDependencies: v });
              }}
              onShowRemoteVersionChange={(v) => {
                setShowRemoteVersion(v);
                updateSettingsFromUser((c) => ({ ...c, showRemoteVersion: v }));
                persistSettings({ showRemoteVersion: v });
              }}
              onCranMirrorChange={(v) => acceptSettingValue("cranMirror", v)}
              onMirrorSelect={(v) => updateSettingsFromUser((c) => ({ ...c, cranMirror: v }))}
              onSaveSettings={persistSettings}
              onThemeChange={handleThemeChange} onFontChange={handleFontChange}
              onCheckUpdates={checkForUpdates}
              onClearCache={async () => {
                try { await invoke("clear_package_cache"); setStatus("包缓存已清除"); }
                catch (error) { setStatus(`缓存清除失败: ${formatError(error)}`); }
              }}
              onExportDiagnostics={async () => {
                try {
                  const diagnostics = await invoke<string>("export_diagnostics");
                  const blob = new Blob([diagnostics], { type: "application/json" });
                  const url = URL.createObjectURL(blob);
                  const a = document.createElement("a");
                  a.href = url;
                  a.download = `r-links-diagnostics-${Date.now()}.json`;
                  a.click();
                  URL.revokeObjectURL(url);
                  setStatus("诊断信息已导出");
                } catch (error) { setStatus(`诊断导出失败: ${formatError(error)}`); }
              }}
              inputRules={inputRules}
              onInputRulesChange={setInputRules}
              onSaveInputRules={saveInputRules}
              inputRulesBusy={inputRulesBusy}
            />
          )}
        </section>
      </main>
    </div>
  );
}

export default App;