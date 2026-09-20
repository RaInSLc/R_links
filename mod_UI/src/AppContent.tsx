import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { useSettings } from "./useSettings";
import { useHistory } from "./useHistory";
import { useSearch } from "./useSearch";
import { useScriptGeneration } from "./useScriptGeneration";
import { AppPages, type AppPagesProps } from "./AppPages";
import { useAppActions } from "./useAppActions";
import { activeInputLineCount, buildInputSmartSuggestions, buildResultSmartSuggestions, classifyInputProfile, countDuplicatePackages, countScriptCommands, formatError, nonEmptyLineBytesExceeds, scriptValueTooLarge, utf8Length, MAX_INPUT_CHARS, MAX_INPUT_LINE_BYTES, MAX_PACKAGE_LINES, type SearchResult } from "./utils";
import { type Ecosystem, type InputRules, type Method, type Settings, type View, defaultInputRules, defaultSettings } from "./types";

type UpdateState = "idle" | "checking" | "available" | "downloading" | "installing" | "readyToRestart" | "upToDate" | "error";

export function AppContent() {
  const [view, setView] = useState<View>("workspace");
  const [currentTheme, setCurrentTheme] = useState(() => localStorage.getItem("theme") || "office");
  const [currentFont, setCurrentFont] = useState(() => localStorage.getItem("fontFamily") || "modern");
  const [currentFontSize, setCurrentFontSize] = useState(() => { const value = Number(localStorage.getItem("fontSize")); return value >= 12 && value <= 20 ? value : 14; });
  const [input, setInput] = useState(() => localStorage.getItem("rlinks_input") || "");
  const [ecosystem, setEcosystem] = useState<Ecosystem>(() => (localStorage.getItem("rlinks_ecosystem") as Ecosystem) || "r");
  const [pipIndex, setPipIndex] = useState(() => localStorage.getItem("rlinks_pip_index") || defaultSettings.pipIndex);
  const [condaChannels, setCondaChannels] = useState(() => (localStorage.getItem("rlinks_conda_channels") || defaultSettings.condaChannels.join("\n")).split("\n"));
  const [rBinaryMirror, setRBinaryMirror] = useState(() => localStorage.getItem("rlinks_r_binary_mirror") || "https://packagemanager.posit.co/cran/latest");
  const [copyWithLineNumbers, setCopyWithLineNumbers] = useState(false);
  const [method, setMethod] = useState<Method>(() => (localStorage.getItem("rlinks_method") as Method) || "auto");
  const [conditional, setConditionalState] = useState(() => localStorage.getItem("rlinks_conditional") !== "0");
  const [installDependencies, setInstallDependenciesState] = useState(() => localStorage.getItem("rlinks_install_deps") !== "0");
  const [showRemoteVersion, setShowRemoteVersionState] = useState(() => localStorage.getItem("rlinks_show_remote_version") !== "0");
  const [verifyInstall, setVerifyInstallState] = useState(() => localStorage.getItem("rlinks_verify_install") === "1");
  const [parallelInstall, setParallelInstallState] = useState(() => localStorage.getItem("rlinks_parallel_install") === "1");
  const [status, setStatus] = useState("就绪");
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateState, setUpdateState] = useState<UpdateState>("idle");
  const [updateMessage, setUpdateMessage] = useState("");
  const [appVersion, setAppVersion] = useState("");
  const [updateVersion, setUpdateVersion] = useState("");
  const [inputRules, setInputRules] = useState<InputRules>(defaultInputRules);
  const [inputRulesBusy, setInputRulesBusy] = useState(false);
  const search = useSearch(setStatus);
  const settingsHook = useSettings(setStatus);
  const historyHook = useHistory(setStatus);
  const {
    results, setResults, logs, setLogs, dependencyGraph, searching,
    openingSearchTabs, searchingRef, hasSearchEvidenceRef, paused,
    togglePauseSearch, cancelSearchPackage, searchDuration, stageTimings,
    startSearch, startBinarySearch, startMultiEcosystemSearch, stopSearch,
    openSearchTabs,
  } = search;
  const {
    settings, showToken, setShowToken, tokenConfigured, settingsBusy,
    updateSettingsFromUser, replaceSettingsFromUser, acceptSettingValue,
    persistSettings, clearSavedToken,
  } = settingsHook;
  const { history, historySearch, setHistorySearch, sanitizeHistoryList, enqueueHistorySave, copyHistoryRecord, deleteHistoryRecord, clearAllHistory } = historyHook;
  const latestInputRef = useRef(localStorage.getItem("rlinks_input") || ""); const copyWithLineNumbersRef = useRef(false);
  useEffect(() => { copyWithLineNumbersRef.current = copyWithLineNumbers; }, [copyWithLineNumbers]);
  const packageCount = useMemo(() => activeInputLineCount(input, inputRules.separators), [input, inputRules.separators]);
  const inputProfile = useMemo(() => classifyInputProfile(input, inputRules.separators), [input, inputRules.separators]);
  const inputTooLarge = utf8Length(input) > MAX_INPUT_CHARS || packageCount > MAX_PACKAGE_LINES || nonEmptyLineBytesExceeds(input, MAX_INPUT_LINE_BYTES);
  const smartSuggestions = useMemo(() => buildInputSmartSuggestions(input, inputProfile, method, { verifyInstall }), [input, inputProfile, method, verifyInstall]);
  const resultSuggestions = useMemo(() => buildResultSmartSuggestions(results, { fullSearch: settings.fullSearch, searching }), [results, settings.fullSearch, searching]);
  const uniqueFoundCount = useMemo(() => new Set(results.filter((result) => result.found).map((result) => result.package)).size, [results]);
  const { script, latestScriptRef, requestSeq, setScript } = useScriptGeneration(
    input, ecosystem, pipIndex, condaChannels, method, conditional,
    installDependencies, showRemoteVersion, verifyInstall, parallelInstall,
    settings, rBinaryMirror, results, inputTooLarge, setStatus,
  );
  const actions = useAppActions({ view, setView, input, setInput, inputProfile, inputRules, inputTooLarge, method, setMethod, ecosystem, pipIndex, rBinaryMirror, conditional, setConditional: (v) => { setConditionalState(v); localStorage.setItem("rlinks_conditional", v ? "1" : "0"); }, installDependencies, setInstallDependencies: (v) => { setInstallDependenciesState(v); localStorage.setItem("rlinks_install_deps", v ? "1" : "0"); }, showRemoteVersion, setShowRemoteVersion: (v) => { setShowRemoteVersionState(v); localStorage.setItem("rlinks_show_remote_version", v ? "1" : "0"); }, verifyInstall, setVerifyInstall: (v) => { setVerifyInstallState(v); localStorage.setItem("rlinks_verify_install", v ? "1" : "0"); }, settings, updateAndPersistSettings: (update) => { let next: Settings | undefined; updateSettingsFromUser((current) => (next = update(current))); if (next) void persistSettings(next); }, searching, searchingRef, hasSearchEvidenceRef, latestInputRef, latestScriptRef, copyWithLineNumbersRef, setLogs, setStatus, requestSeq, setScript, startSearch, startBinarySearch, startMultiEcosystemSearch, stopSearch, sanitizeHistoryList, enqueueHistorySave, copyHistoryRecord, deleteHistoryRecord, clearAllHistory, setInputRulesBusy });
  useEffect(() => { localStorage.setItem("rlinks_input", input); localStorage.setItem("rlinks_method", method); }, [input, method]);
  useEffect(() => { invoke<InputRules>("load_input_rules").then(setInputRules).catch(() => {}); import("@tauri-apps/api/app").then(({ getVersion }) => getVersion()).then(setAppVersion).catch(() => setAppVersion("0.1.9")); }, []);
  const initialInput = useRef(input);
  useEffect(() => {
    const requestedInput = initialInput.current;
    if (!requestedInput.trim()) return;
    let active = true;
    invoke<SearchResult[]>("load_cached_results", { input: requestedInput })
      .then((cached) => {
        if (active && latestInputRef.current === requestedInput && cached.length > 0) {
          setResults(cached);
          hasSearchEvidenceRef.current = true;
        }
      })
      .catch(() => {});
    return () => { active = false; };
  }, [setResults, hasSearchEvidenceRef]);
  useEffect(() => { document.documentElement.setAttribute("data-theme", currentTheme); document.documentElement.setAttribute("data-font", currentFont); document.documentElement.style.fontSize = `${currentFontSize}px`; localStorage.setItem("fontSize", String(currentFontSize)); }, [currentTheme, currentFont, currentFontSize]);
  useEffect(() => { document.title = searching && packageCount > 0 ? `R Package Center — 检索中 ${results.filter((r) => r.found).length}/${packageCount}` : results.length ? `R Package Center — ${new Set(results.filter((r) => r.found).map((r) => r.package)).size}/${packageCount} 已验证` : "R Package Center"; }, [searching, packageCount, results]);
  const update = async () => {
    setCheckingUpdate(true); setUpdateState("checking"); setUpdateMessage("正在检查更新...");
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const found = await check({ timeout: 20000, proxy: settings.proxy.trim() || undefined });
      if (found) {
        setUpdateVersion(found.version);
        setUpdateState("available");
        setUpdateMessage(`发现新版本 ${found.version}，正在下载并安装...`);
        await found.downloadAndInstall(() => {}, { timeout: 20000 });
        setUpdateState("readyToRestart");
        setUpdateMessage("更新安装成功！请手动关闭并重启应用以生效。");
      } else {
        setUpdateState("upToDate");
        setUpdateMessage("当前已是最新版本");
      }
    } catch (error) {
      setUpdateState("error");
      const message = formatError(error);
      setUpdateMessage(
        message.includes("valid release JSON")
          ? "检查更新失败：GitHub Release 缺少 latest.json 自动更新清单；请先使用安装包手动更新，或重新发布包含清单的版本。"
          : message.includes("error sending request") || message.includes("Network Error") || message.includes("Failed to fetch") || message.includes("timeout")
            ? "检查更新失败：无法连接 GitHub 更新清单。请确认网络可访问 GitHub，或在网络设置中配置代理后重试；也可以前往 GitHub Releases 手动下载安装包。"
            : `检查更新失败: ${message}`,
      );
    } finally {
      setCheckingUpdate(false);
    }
  };
  const updateAndPersistSettings = (update: (current: Settings) => Settings) => {
    let next: Settings | undefined;
    updateSettingsFromUser((current) => (next = update(current)));
    if (next) void persistSettings(next);
  };
  const checkedProps: AppPagesProps = {
    view, setView, status, searching, packageCount, results, history, historySearch,
    foundCount: uniqueFoundCount,
    onHistorySearchChange: setHistorySearch, onCopyRecord: copyHistoryRecord, onDeleteRecord: deleteHistoryRecord, onClearAll: clearAllHistory,
    summaryProgress: packageCount ? Math.min(100, uniqueFoundCount / packageCount * 100) : 0,
    input, inputTooLarge, inputProfile, method, conditional, installDependencies, parallelInstall, ecosystem, pipIndex, condaChannels, rBinaryMirror,
    showRemoteVersion, verifyInstall, settings, smartSuggestions, script,
    scriptTooLarge: scriptValueTooLarge(script), scriptCommandCount: countScriptCommands(script), duplicateCount: countDuplicatePackages(input),
    paused, openingSearchTabs, logs, dependencyGraph, resultSuggestions, searchDuration, stageTimings, inputRules, inputRulesBusy,
    tokenConfigured, showToken, settingsBusy, currentTheme, currentFont, currentFontSize, checkingUpdate, updateState, updateMessage, appVersion, updateVersion,
    copyWithLineNumbers, pinnedMethods: settings.pinnedMethods, ...actions, setStatus, cancelSearchPackage, updateAndPersistSettings,
    onInputChange: actions.acceptInputValue, onPaste: actions.pasteInput, onClear: () => actions.acceptInputValue("", "manual"),
    onOpenSearchTabs: () => { void openSearchTabs(input, inputTooLarge, ecosystem, inputRules.separators); },
    onStopSearch: () => { void stopSearch(); }, onTogglePause: () => { void togglePauseSearch(); }, onMethodChange: setMethod,
    onEcosystemChange: (value) => { setEcosystem(value); localStorage.setItem("rlinks_ecosystem", value); },
    onPipIndexChange: (value) => { setPipIndex(value); localStorage.setItem("rlinks_pip_index", value); updateAndPersistSettings((current) => ({ ...current, pipIndex: value })); },
    onCondaChannelsChange: (value) => { setCondaChannels(value); localStorage.setItem("rlinks_conda_channels", value.join("\n")); updateAndPersistSettings((current) => ({ ...current, condaChannels: value })); },
    onRBinaryMirrorChange: (value) => { setRBinaryMirror(value); localStorage.setItem("rlinks_r_binary_mirror", value); },
    onPinnedMethodsChange: (value) => updateAndPersistSettings((current) => ({ ...current, pinnedMethods: value })),
    onApplySmartSuggestion: (suggestion) => { if (suggestion.action === "enableVerify") setVerifyInstallState(true); else if (suggestion.value) actions.acceptInputValue(suggestion.value, "manual"); setStatus(`已应用智能建议：${suggestion.title}`); },
    onClearLogs: () => setLogs([]), onStatusChange: setStatus,
    onProxyChange: (value) => acceptSettingValue("proxy", value), onTokenChange: (value) => acceptSettingValue("githubToken", value), onTokenToggle: () => setShowToken((value) => !value), onClearToken: clearSavedToken,
    onFullSearchChange: (value) => updateAndPersistSettings((current) => ({ ...current, fullSearch: value })),
    onSearchConcurrencyChange: (value) => updateAndPersistSettings((current) => ({ ...current, searchConcurrency: value })),
    onArchiveGithubMajorGapChange: (value) => updateAndPersistSettings((current) => ({ ...current, archiveGithubMajorGap: value })),
    onUseCacheChange: (value) => updateAndPersistSettings((current) => ({ ...current, useCache: value })),
    onUseFilterChange: (value) => updateAndPersistSettings((current) => ({ ...current, useFilter: value })),
    onMaxCacheEntriesChange: (value) => updateAndPersistSettings((current) => ({ ...current, maxCacheEntries: value })),
    onConditionalChange: (value) => { setConditionalState(value); localStorage.setItem("rlinks_conditional", value ? "1" : "0"); updateAndPersistSettings((current) => ({ ...current, conditional: value })); },
    onInstallDependenciesChange: (value) => { setInstallDependenciesState(value); localStorage.setItem("rlinks_install_deps", value ? "1" : "0"); updateAndPersistSettings((current) => ({ ...current, installDependencies: value })); },
    onShowRemoteVersionChange: (value) => { setShowRemoteVersionState(value); localStorage.setItem("rlinks_show_remote_version", value ? "1" : "0"); updateAndPersistSettings((current) => ({ ...current, showRemoteVersion: value })); },
    onVerifyInstallChange: (value) => { setVerifyInstallState(value); localStorage.setItem("rlinks_verify_install", value ? "1" : "0"); updateAndPersistSettings((current) => ({ ...current, verifyInstall: value })); },
    onParallelInstallChange: (value) => { setParallelInstallState(value); localStorage.setItem("rlinks_parallel_install", value ? "1" : "0"); updateAndPersistSettings((current) => ({ ...current, parallelInstall: value })); },
    onCopyWithLineNumbersChange: setCopyWithLineNumbers,
    onCranMirrorChange: (value) => acceptSettingValue("cranMirror", value), onRLibPathChange: (value) => acceptSettingValue("rLibPath", value),
    onMirrorSelect: (value) => updateAndPersistSettings((current) => ({ ...current, cranMirror: value })),
    onResolveDependenciesChange: (value) => updateAndPersistSettings((current) => ({ ...current, resolveDependencies: value })),
    onIncludeLightDependenciesChange: (value) => updateAndPersistSettings((current) => ({ ...current, includeLightDependencies: value })),
    onMaxDependencyDepthChange: (value) => updateAndPersistSettings((current) => ({ ...current, maxDependencyDepth: value })),
    onMaxDependencyNodesChange: (value) => updateAndPersistSettings((current) => ({ ...current, maxDependencyNodes: value })),
    onSaveSettings: persistSettings, replaceSettingsFromUser, persistSettings,
    onThemeChange: setCurrentTheme, onFontChange: setCurrentFont, onFontSizeChange: setCurrentFontSize, onCheckUpdates: update,
    onClearCache: async () => { try { await invoke("clear_package_cache"); setStatus("包缓存已清除"); } catch (error) { setStatus(`缓存清除失败: ${formatError(error)}`); } },
    onInputRulesChange: setInputRules, onReplaceInputRules: (next) => { setInputRules(next); void actions.saveInputRules(next); },
    uniqueFoundCount,
    onExportDiagnostics: async () => {
      try {
        const content = await invoke<string>("export_diagnostics");
        const url = URL.createObjectURL(new Blob([content], { type: "application/json;charset=utf-8" }));
        const anchor = document.createElement("a"); anchor.href = url; anchor.download = "diagnostics.json"; anchor.click(); URL.revokeObjectURL(url);
        setStatus("诊断信息已导出");
      } catch (error) { setStatus(`诊断导出失败: ${formatError(error)}`); }
    },
  };
  return <AppPages {...checkedProps} />;
}
