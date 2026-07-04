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
  buildInputSmartSuggestions, buildResultSmartSuggestions,
  countScriptCommands, countDuplicatePackages,
  MAX_INPUT_CHARS, MAX_INPUT_LINE_BYTES, MAX_PACKAGE_LINES,
  MAX_SCRIPT_CHARS, MAX_HISTORY_RECORDS, utf8Length,
  type HistoryRecord, type SearchResult,
} from "./utils";
import { type View, type Method, type InputRules, defaultInputRules, defaultSettings, defaultPinnedMethods } from "./types";

function App() {
  const [view, setView] = useState<View>("workspace");
  const [currentTheme, setCurrentTheme] = useState(() => localStorage.getItem("theme") || "office");
  const [currentFont, setCurrentFont] = useState(() => localStorage.getItem("fontFamily") || "modern");
  const [currentFontSize, setCurrentFontSize] = useState(() => {
    const v = Number(localStorage.getItem("fontSize"));
    return (v >= 12 && v <= 20) ? v : 14;
  });
  const [input, setInput] = useState(() => localStorage.getItem("rlinks_input") || "");
  const [method, setMethod] = useState<Method>(() => {
    const stored = localStorage.getItem("rlinks_method");
    if (stored === "auto" || stored === "devtools" || stored === "remotes" ||
        stored === "github" || stored === "base" || stored === "version" ||
        stored === "biocManager" || stored === "checkSystem") return stored as Method;
    return "auto";
  });
  const [conditional, setConditionalState] = useState(() => {
    const s = localStorage.getItem("rlinks_conditional");
    return s === null ? true : s === "1";
  });
  const [installDependencies, setInstallDependenciesState] = useState(() => {
    const s = localStorage.getItem("rlinks_install_deps");
    return s === null ? true : s === "1";
  });
  const [showRemoteVersion, setShowRemoteVersionState] = useState(() => {
    const s = localStorage.getItem("rlinks_show_remote_version");
    return s === null ? true : s === "1";
  });
  const [verifyInstall, setVerifyInstallState] = useState(() => {
    return localStorage.getItem("rlinks_verify_install") === "1";
  });
  const [pinnedMethods, setPinnedMethods] = useState<Method[]>(() => {
    try {
      const stored = localStorage.getItem("rlinks_pinned_methods");
      if (stored) {
        const parsed = JSON.parse(stored);
        if (Array.isArray(parsed) && parsed.length >= 1) return parsed;
      }
    } catch {}
    return [...defaultPinnedMethods];
  });
  const [script, setScriptState] = useState("等待输入...");
  const [status, setStatus] = useState("就绪");
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateMessage, setUpdateMessage] = useState("");
  const [inputRules, setInputRules] = useState<InputRules>(defaultInputRules);
  const [inputRulesBusy, setInputRulesBusy] = useState(false);

  function setConditional(v: boolean) { setConditionalState(v); localStorage.setItem("rlinks_conditional", v ? "1" : "0"); }
  function setInstallDependencies(v: boolean) { setInstallDependenciesState(v); localStorage.setItem("rlinks_install_deps", v ? "1" : "0"); }
  function setShowRemoteVersion(v: boolean) { setShowRemoteVersionState(v); localStorage.setItem("rlinks_show_remote_version", v ? "1" : "0"); }
  function setVerifyInstall(v: boolean) { setVerifyInstallState(v); localStorage.setItem("rlinks_verify_install", v ? "1" : "0"); }

  const latestInputRef = useRef(localStorage.getItem("rlinks_input") || "");
  const latestScriptRef = useRef("等待输入...");
  const scriptRequestSeq = useRef(0);

  const search = useSearch(setStatus);
  const settingsHook = useSettings(setStatus);
  const historyHook = useHistory(setStatus);

  const { results, setResults, logs, setLogs, dependencyGraph,
    searching, openingSearchTabs, searchingRef, hasSearchEvidenceRef,
    searchDuration,
    startSearch, stopSearch, openSearchTabs } = search;
  const { settings, showToken, setShowToken,
    tokenConfigured, settingsBusy, updateSettingsFromUser,
    acceptSettingValue, persistSettings, clearSavedToken } = settingsHook;
  const { history, historySearch, setHistorySearch,
    sanitizeHistoryList, enqueueHistorySave,
    copyHistoryRecord, deleteHistoryRecord, clearAllHistory } = historyHook;

  function setScript(next: string) {
    latestScriptRef.current = next;
    setScriptState(next);
  }

  const packageCount = useMemo(() => activeInputLineCount(input), [input]);
  const inputProfile = useMemo(() => classifyInputProfile(input), [input]);
  const smartSuggestions = useMemo(
    () => buildInputSmartSuggestions(input, inputProfile, method, { verifyInstall }),
    [input, inputProfile, method, verifyInstall],
  );
  const resultSuggestions = useMemo(
    () => buildResultSmartSuggestions(results, { fullSearch: settings.fullSearch, searching }),
    [results, settings.fullSearch, searching],
  );
  const inputBytes = useMemo(() => utf8Length(input), [input]);
  const inputTooLarge =
    inputBytes > MAX_INPUT_CHARS ||
    packageCount > MAX_PACKAGE_LINES ||
    nonEmptyLineBytesExceeds(input, MAX_INPUT_LINE_BYTES);
  const scriptTooLarge = useMemo(() => scriptValueTooLarge(script), [script]);
  const scriptCommandCount = useMemo(() => countScriptCommands(script), [script]);
  const duplicateCount = useMemo(() => countDuplicatePackages(input), [input]);
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
    localStorage.setItem("rlinks_input", input);
  }, [input]);

  useEffect(() => {
    localStorage.setItem("rlinks_pinned_methods", JSON.stringify(pinnedMethods));
  }, [pinnedMethods]);

  useEffect(() => {
    localStorage.setItem("rlinks_method", method);
  }, [method]);

  useEffect(() => {
    if (!input.trim()) return;
    invoke<SearchResult[]>("load_cached_results", { input })
      .then((cached) => {
        if (cached.length > 0) {
          setResults(cached);
          hasSearchEvidenceRef.current = true;
        }
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", currentTheme);
  }, [currentTheme]);
  useEffect(() => {
    document.documentElement.setAttribute("data-font", currentFont);
  }, [currentFont]);
  useEffect(() => {
    document.documentElement.style.fontSize = `${currentFontSize}px`;
    localStorage.setItem("fontSize", String(currentFontSize));
  }, [currentFontSize]);

  const handleThemeChange = (theme: string) => {
    setCurrentTheme(theme);
    localStorage.setItem("theme", theme);
  };
  const handleFontChange = (font: string) => {
    setCurrentFont(font);
    localStorage.setItem("fontFamily", font);
  };
  const handleFontSizeChange = (size: number) => {
    setCurrentFontSize(size);
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
      setLogs([]);
    }
    latestInputRef.current = value;
    setInput(value);
    if (clearsSearchEvidence && source === "manual") {
      setStatus("输入已变更，检索日志已清除（已验证的来源信息保留）");
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
          setStatus(result === "cleared" ? "已从剪贴板粘贴，检索日志已清除（来源信息保留）" : "已从剪贴板粘贴");
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
        options: { method, conditional, installDependencies, mirror: settings.cranMirror, appendVerify: verifyInstall },
        results,
        showRemoteVersion,
      })
        .then((next) => { if (active && seq === scriptRequestSeq.current) setScript(next); })
        .catch((error) => { if (active && seq === scriptRequestSeq.current) setStatus(`生成失败: ${formatError(error)}`); });
    }, 120);
    return () => { active = false; window.clearTimeout(timer); };
  }, [input, method, conditional, installDependencies, showRemoteVersion, verifyInstall, settings.cranMirror, results, inputTooLarge]);

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

  function downloadScript() {
    const snapshot = latestScriptRef.current;
    if (!snapshot || snapshot === "等待输入...") return;
    const blob = new Blob([snapshot], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "install_packages.R";
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    setStatus("已下载 R 脚本文件");
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

  function handleTempFilter(text: string, mode: "chars" | "lines") {
    if (!text) return;
    let nextValue = "";
    if (mode === "chars") {
      try {
        const regex = new RegExp(text, "g");
        nextValue = input.replace(regex, "");
      } catch {
        nextValue = input.split(text).join("");
      }
    } else {
      const lines = input.split(/\r?\n/);
      let regex: RegExp | null = null;
      try {
        regex = new RegExp(text);
      } catch {}
      const filtered = lines.filter((line) => {
        if (regex) {
          return !regex.test(line);
        }
        return !line.includes(text);
      });
      nextValue = filtered.join("\n");
    }
    acceptInputValue(nextValue, "manual");
    setStatus(`已临时过滤: ${mode === "chars" ? "剔除匹配字符" : "剔除匹配整行"}`);
  }

  function handleStartSearch() {
    startSearch(input, settings, inputTooLarge, () => setView("report"), () => setMethod("auto"));
  }

  useEffect(() => {
    function onKeydown(e: KeyboardEvent) {
      if (view !== "workspace") return;
      if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
        e.preventDefault();
        if (searching) { stopSearch(); } else if (input.trim() && !inputTooLarge) { handleStartSearch(); }
      } else if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === "C" || e.key === "c")) {
        e.preventDefault();
        copyScript();
      } else if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        downloadScript();
      } else if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === "K" || e.key === "k")) {
        e.preventDefault();
        if (!searching && input.trim()) { setInput(""); }
      }
    }
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  }, [view, searching, input, inputTooLarge]);

  useEffect(() => {
    function onKeydown(e: KeyboardEvent) {
      if (!(e.ctrlKey || e.metaKey) || e.shiftKey || e.altKey) return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if (e.key === "1") { e.preventDefault(); setView("workspace"); }
      else if (e.key === "2") { e.preventDefault(); setView("report"); }
      else if (e.key === "3") { e.preventDefault(); setView("history"); }
      else if (e.key === "4") { e.preventDefault(); setView("settings"); }
    }
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  }, []);

  useEffect(() => {
    const base = "R Package Center";
    if (searching && packageCount > 0) {
      document.title = `${base} — 检索中 ${foundCount}/${packageCount}`;
    } else if (results.length > 0) {
      document.title = `${base} — ${uniqueFoundCount}/${packageCount} 已验证`;
    } else {
      document.title = base;
    }
  }, [searching, foundCount, packageCount, results.length, uniqueFoundCount]);

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
          <NavButton active={view === "workspace"} label="工作台" code="01" onClick={() => setView("workspace")} title="Ctrl+1" />
          <NavButton active={view === "report"} label="检索报告" code="02" badge={results.length} onClick={() => setView("report")} title="Ctrl+2" />
          <NavButton active={view === "history"} label="命令历史" code="03" badge={history.length} onClick={() => setView("history")} title="Ctrl+3" />
          <NavButton active={view === "settings"} label="网络设置" code="04" onClick={() => setView("settings")} title="Ctrl+4" />
        </nav>
        <div className="sidebar-summary">
          <span>当前任务</span>
          <strong>{searching ? `检索中 ${foundCount}/${packageCount}（${packageCount > 0 ? Math.round((foundCount / packageCount) * 100) : 0}%）` : `${packageCount} 个输入`}</strong>
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
              showRemoteVersion={showRemoteVersion} verifyInstall={verifyInstall}
              settings={settings}
              smartSuggestions={smartSuggestions}
              script={script} scriptTooLarge={scriptTooLarge}
              scriptCommandCount={scriptCommandCount}
              duplicateCount={duplicateCount}
              searching={searching} openingSearchTabs={openingSearchTabs}
              onInputChange={acceptInputValue} onPaste={pasteInput}
              onClear={() => acceptInputValue("", "manual")}
              onOpenSearchTabs={() => openSearchTabs(input, inputTooLarge)}
              onStartSearch={handleStartSearch} onStopSearch={stopSearch}
              onMethodChange={setMethod}
              pinnedMethods={pinnedMethods}
              onPinnedMethodsChange={setPinnedMethods}
              onApplySmartSuggestion={(suggestion) => {
                if (suggestion.action === "enableVerify") {
                  setVerifyInstall(true);
                  setStatus(`已应用智能建议：${suggestion.title}`);
                } else if (suggestion.action === "replaceInput" && suggestion.value) {
                  const result = acceptInputValue(suggestion.value, "manual");
                  if (result !== "rejected") setStatus(`已应用智能建议：${suggestion.title}`);
                } else if (suggestion.method && methodSupportsInput(suggestion.method, inputProfile)) {
                  setMethod(suggestion.method as Method);
                  setStatus(`已应用智能建议：${suggestion.title}`);
                }
              }}
              onConditionalChange={setConditional}
              onInstallDependenciesChange={setInstallDependencies}
              onShowRemoteVersionChange={setShowRemoteVersion}
              onVerifyInstallChange={setVerifyInstall}
              onFullSearchChange={(v) => updateSettingsFromUser((c) => ({ ...c, fullSearch: v }))}
              onUseCacheChange={(v) => {
                updateSettingsFromUser((c) => ({ ...c, useCache: v }));
                persistSettings({ useCache: v });
              }}
              onTempFilter={handleTempFilter}
              onCopyScript={copyScript} onCleanComments={cleanComments}
              onDownloadScript={downloadScript}
              isMethodDisabled={isMethodDisabled}
            />
          )}
          {view === "report" && (
            <ReportView
              results={results} logs={logs} dependencyGraph={dependencyGraph}
              packageCount={packageCount} uniqueFoundCount={uniqueFoundCount}
              smartSuggestions={resultSuggestions}
              searching={searching} onClearLogs={() => setLogs([])}
              searchDuration={searchDuration}
              onStatusChange={setStatus}
              onApplySmartSuggestion={(suggestion) => {
                if (suggestion.action === "openSettings") {
                  setView("settings");
                  setStatus(`已应用智能建议：${suggestion.title}`);
                } else if (suggestion.action === "enableFullSearch") {
                  updateSettingsFromUser((current) => ({ ...current, fullSearch: true }));
                  persistSettings({ fullSearch: true });
                  setStatus(`已应用智能建议：${suggestion.title}`);
                } else if (suggestion.action === "retrySearch") {
                  setStatus(`已应用智能建议：${suggestion.title}`);
                  handleStartSearch();
                }
              }}
              onRetryMissing={(packages) => {
                acceptInputValue(packages.join("\n"), "manual");
                setView("workspace");
                setStatus(`已回填 ${packages.length} 个未找到的包名，可重新检索`);
              }}
            />
          )}
          {view === "history" && (
            <HistoryView
              history={history} historySearch={historySearch}
              onHistorySearchChange={setHistorySearch}
              onApplyRecord={applyHistoryRecord}
              onCopyRecord={copyHistoryRecord}
              onDeleteRecord={deleteHistoryRecord}
              onClearAll={clearAllHistory}
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
              onResolveDependenciesChange={(v) => {
                updateSettingsFromUser((c) => ({ ...c, resolveDependencies: v }));
                persistSettings({ resolveDependencies: v });
              }}
              onIncludeLightDependenciesChange={(v) => {
                updateSettingsFromUser((c) => ({ ...c, includeLightDependencies: v }));
                persistSettings({ includeLightDependencies: v });
              }}
              onMaxDependencyDepthChange={(v) => {
                updateSettingsFromUser((c) => ({ ...c, maxDependencyDepth: v }));
              }}
              onMaxDependencyNodesChange={(v) => {
                updateSettingsFromUser((c) => ({ ...c, maxDependencyNodes: v }));
              }}
              onSaveSettings={persistSettings}
              onThemeChange={handleThemeChange} onFontChange={handleFontChange}
              currentFontSize={currentFontSize} onFontSizeChange={handleFontSizeChange}
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