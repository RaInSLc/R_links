import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  readText,
  writeText,
} from "@tauri-apps/plugin-clipboard-manager";
import "./App.css";
import { NavButton, PanelHeader, Toggle, Metric, EmptyState } from "./components";
import { appendBounded, asArray, asRecord, formatError, upsertBoundedResult,
  inputValueTooLarge, scriptValueTooLarge, settingsValueTooLargeOrUnsafe,
  githubTokenTextAllowed, settingsFieldLabel, activeInputLineCount,
  nonEmptyLineBytesExceeds,
  mapBounded, MAX_HISTORY_RECORDS, MAX_INPUT_CHARS,
  MAX_INPUT_LINE_BYTES, MAX_PACKAGE_LINES, MAX_RESULT_FIELD_CHARS,
  MAX_SCRIPT_CHARS, MAX_SEARCH_LOGS, MAX_SEARCH_RESULTS, MAX_SEARCH_TABS,
  MAX_TOKEN_CHARS,
  BROWSER_SEARCH_CONFIRM_THRESHOLD, HISTORY_LOAD_WAIT_TIMEOUT_MS,
  safeRunId, safeStatusText,
  sanitizePublicSettings, sanitizeHistoryRecord, sanitizeSearchResponse,
  sanitizeSearchResult, collectBrowserSearchNames,
  classifyInputProfile, methodSupportsInput, nextSearchRunId,
  utf8Length,
  type HistoryRecord, type PublicSettings, type SearchResponse, type SearchResult } from "./utils";
import {
  type View, type Method, type Settings,
  type SearchLogEvent, type SearchProgressEvent,
  defaultSettings, methods, mirrors, sourceNames,
} from "./types";

function App() {
  const [view, setView] = useState<View>("workspace");
  const [currentTheme, setCurrentTheme] = useState(() => localStorage.getItem("theme") || "office");
  const [currentFont, setCurrentFont] = useState(() => localStorage.getItem("fontFamily") || "modern");
  const [input, setInput] = useState("");
  const [method, setMethod] = useState<Method>("auto");
  const [historySearch, setHistorySearch] = useState("");

  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateMessage, setUpdateMessage] = useState("");

  async function checkForUpdates() {
    setCheckingUpdate(true);
    setUpdateMessage("正在检查更新...");
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      const update = await check();
      if (update) {
        setUpdateMessage(`发现新版本 ${update.version}，正在下载并安装...`);
        let downloaded = 0;
        let contentLength = 0;
        await update.downloadAndInstall((event) => {
          switch (event.event) {
            case 'Started':
              contentLength = event.data?.contentLength || 0;
              setUpdateMessage(`正在下载新版本...`);
              break;
            case 'Progress':
              downloaded += event.data?.chunkLength || 0;
              if (contentLength > 0) {
                const percent = Math.round((downloaded / contentLength) * 100);
                setUpdateMessage(`正在下载... ${percent}%`);
              }
              break;
            case 'Finished':
              setUpdateMessage(`下载完成，正在安装...`);
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
  const [conditional, setConditional] = useState(true);
  const [installDependencies, setInstallDependencies] = useState(true);
  const [showRemoteVersion, setShowRemoteVersion] = useState(true);
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [script, setScriptState] = useState("等待输入...");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [history, setHistoryState] = useState<HistoryRecord[]>([]);
  const [searching, setSearching] = useState(false);
  const [status, setStatus] = useState("就绪");
  const [showToken, setShowToken] = useState(false);
  const [tokenConfigured, setTokenConfigured] = useState(false);
  const [settingsBusy, setSettingsBusy] = useState(false);
  const [openingSearchTabs, setOpeningSearchTabs] = useState(false);
  const activeSearchRunId = useRef(0);
  const searchingRef = useRef(false);
  const latestInputRef = useRef("");
  const hasSearchEvidenceRef = useRef(false);
  const browserOpenInProgress = useRef(false);
  const scriptRequestSeq = useRef(0);
  const settingsActionSeq = useRef(0);
  const settingsBusyRef = useRef(false);
  const latestScriptRef = useRef("等待输入...");
  const latestHistoryRef = useRef<HistoryRecord[]>([]);
  const historyActionSeq = useRef(0);
  const historySaveQueue = useRef(Promise.resolve());
  const historyLoadResolveRef = useRef<() => void>(() => undefined);
  const historyLoadReadyRef = useRef<Promise<void> | null>(null);
  if (historyLoadReadyRef.current === null) {
    historyLoadReadyRef.current = new Promise<void>((resolve) => {
      historyLoadResolveRef.current = resolve;
    });
  }

  function setScript(nextScript: string) {
    latestScriptRef.current = nextScript;
    setScriptState(nextScript);
  }

  function acceptInputValue(value: string, source: "manual" | "clipboard") {
    if (searchingRef.current) {
      setStatus("检索期间不能修改输入，请先停止当前任务");
      return "rejected";
    }
    if (inputValueTooLarge(value)) {
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

  function acceptSettingValue(field: keyof Pick<Settings, "proxy" | "githubToken" | "cranMirror">, value: string) {
    const nextValue = field === "proxy" ? value : value.trim();
    const label = settingsFieldLabel(field);
    const limit = field === "githubToken" ? MAX_TOKEN_CHARS : MAX_RESULT_FIELD_CHARS;
    if (settingsValueTooLargeOrUnsafe(nextValue, limit)) {
      setStatus(`${label}包含非法字符或长度过长，最多允许 ${limit} 字节`);
      return false;
    }
    if (field === "githubToken" && !githubTokenTextAllowed(nextValue)) {
      setStatus("GitHub Token 仅允许可见 ASCII 字符，不能包含空白字符");
      return false;
    }
    updateSettingsFromUser((current) => ({ ...current, [field]: nextValue }));
    return true;
  }

  function updateSettingsFromUser(update: (current: Settings) => Settings) {
    settingsActionSeq.current += 1;
    setSettings(update);
  }

  function beginSettingsOperation() {
    if (settingsBusyRef.current) {
      setStatus("设置操作正在进行，请稍候");
      return false;
    }
    settingsBusyRef.current = true;
    setSettingsBusy(true);
    return true;
  }

  function endSettingsOperation() {
    settingsBusyRef.current = false;
    setSettingsBusy(false);
  }

  function sanitizeHistoryList(nextHistory: unknown) {
    return mapBounded(asArray(nextHistory), MAX_HISTORY_RECORDS, sanitizeHistoryRecord);
  }

  function setHistory(nextHistory: unknown) {
    const cleanHistory = sanitizeHistoryList(nextHistory);
    latestHistoryRef.current = cleanHistory;
    setHistoryState(cleanHistory);
  }

  function loadInitialHistory(nextHistory: unknown) {
    const cleanHistory = sanitizeHistoryList(nextHistory);
    latestHistoryRef.current = cleanHistory;
    if (historyActionSeq.current === 0) {
      setHistoryState(cleanHistory);
    }
  }

  const packageCount = useMemo(() => activeInputLineCount(input), [input]);
  const inputProfile = useMemo(() => classifyInputProfile(input), [input]);
  const inputBytes = useMemo(() => utf8Length(input), [input]);
  const inputTooLarge =
    inputBytes > MAX_INPUT_CHARS ||
    packageCount > MAX_PACKAGE_LINES ||
    nonEmptyLineBytesExceeds(input, MAX_INPUT_LINE_BYTES);
  const scriptTooLarge = useMemo(() => scriptValueTooLarge(script), [script]);
  const foundCount = results.filter((result) => result.found).length;
  const uniqueFoundCount = new Set(
    results.filter((result) => result.found).map((result) => result.package),
  ).size;
  const summaryProgress = packageCount
    ? Math.min(100, (uniqueFoundCount / packageCount) * 100)
    : 0;

  useEffect(() => {
    let active = true;
    const settingsLoadSeq = settingsActionSeq.current;
    invoke<PublicSettings>("load_settings")
      .then((savedSettings) => {
        if (!active || settingsLoadSeq !== settingsActionSeq.current) {
          return;
        }
        const cleanSettings = sanitizePublicSettings(savedSettings);
        setSettings({
          proxy: cleanSettings.proxy,
          githubToken: "",
          cranMirror: cleanSettings.cranMirror,
          fullSearch: cleanSettings.fullSearch,
        });
        setTokenConfigured(cleanSettings.githubTokenConfigured);
      })
      .catch((error) => {
        if (active && settingsLoadSeq === settingsActionSeq.current) {
          setStatus(`设置加载失败: ${formatError(error)}`);
        }
      });

    invoke<HistoryRecord[]>("load_history")
      .then((nextHistory) => {
        if (active) {
          loadInitialHistory(nextHistory);
        }
      })
      .catch((error) => {
        if (active) {
          setStatus(`历史加载失败: ${formatError(error)}`);
        }
      })
      .finally(() => historyLoadResolveRef.current());
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let active = true;
    const unlistenLog = listen<SearchLogEvent>(
      "search-log",
      (event) => {
        const payload = asRecord(event.payload);
        if (!active || safeRunId(payload.runId) !== activeSearchRunId.current) {
          return;
        }
        hasSearchEvidenceRef.current = true;
        setLogs((current) => appendBounded(current, safeStatusText(payload.message), MAX_SEARCH_LOGS));
      },
    ).catch((error) => {
      if (active) {
        setStatus(`检索日志监听失败: ${formatError(error)}`);
      }
      return () => undefined;
    });
    const unlistenProgress = listen<SearchProgressEvent>(
      "search-progress",
      (event) => {
        const payload = asRecord(event.payload);
        if (!active || safeRunId(payload.runId) !== activeSearchRunId.current) {
          return;
        }
        hasSearchEvidenceRef.current = true;
        setResults((current) =>
          upsertBoundedResult(current, sanitizeSearchResult(payload.result), MAX_SEARCH_RESULTS),
        );
      },
    ).catch((error) => {
      if (active) {
        setStatus(`检索进度监听失败: ${formatError(error)}`);
      }
      return () => undefined;
    });
    return () => {
      active = false;
      void unlistenLog.then((unlisten) => unlisten());
      void unlistenProgress.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    let active = true;
    const timer = window.setTimeout(() => {
      const requestSeq = scriptRequestSeq.current + 1;
      scriptRequestSeq.current = requestSeq;
      if (inputTooLarge) {
        setScript("输入超出限制，无法生成脚本。");
        return;
      }
      invoke<string>("generate_script", {
        input,
        options: {
          method,
          conditional,
          installDependencies,
          mirror: settings.cranMirror,
        },
        results,
        showRemoteVersion,
      })
        .then((nextScript) => {
          if (active && requestSeq === scriptRequestSeq.current) {
            setScript(nextScript);
          }
        })
        .catch((error) => {
          if (active && requestSeq === scriptRequestSeq.current) {
            setStatus(`生成失败: ${formatError(error)}`);
          }
        });
    }, 120);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [
    input,
    method,
    conditional,
    installDependencies,
    showRemoteVersion,
    settings.cranMirror,
    results,
    inputTooLarge,
  ]);

  useEffect(() => {
    if (inputProfile.total === 0 || methodSupportsInput(method, inputProfile)) {
      return;
    }
    if (inputProfile.archiveUrls === inputProfile.total) {
      setMethod("remotes");
    } else if (inputProfile.repositories === inputProfile.total) {
      setMethod("github");
    } else {
      setMethod("auto");
    }
  }, [inputProfile, method]);

  async function startSearch() {
    if (!input.trim() || searchingRef.current || inputTooLarge) {
      if (inputTooLarge) {
        setStatus(`输入超出限制或包含非法字符：最多 ${MAX_PACKAGE_LINES} 行、总计 ${MAX_INPUT_CHARS} 字节、单行 ${MAX_INPUT_LINE_BYTES} 字节`);
      }
      return;
    }
    searchingRef.current = true;
    setSearching(true);
    const runId = nextSearchRunId();
    activeSearchRunId.current = runId;
    hasSearchEvidenceRef.current = false;
    setResults([]);
    setLogs([]);
    setStatus("正在检索包来源");
    setView("report");
    try {
      const response = await invoke<SearchResponse>("start_search", {
        runId,
        input,
        settings,
      });
      const cleanResponse = sanitizeSearchResponse(response);
      if (cleanResponse.runId !== activeSearchRunId.current) {
        return;
      }
      hasSearchEvidenceRef.current =
        cleanResponse.results.length > 0 || cleanResponse.logs.length > 0;
      setResults(cleanResponse.results);
      setLogs(cleanResponse.logs);
      setStatus(cleanResponse.stopped ? "检索任务已停止" : "检索完成，脚本已自动刷新");
      if (!cleanResponse.stopped) {
        setMethod("auto");
      }
    } catch (error) {
      if (runId === activeSearchRunId.current) {
        setStatus(`检索失败: ${formatError(error)}`);
      }
    } finally {
      if (runId === activeSearchRunId.current) {
        setSearching(false);
        searchingRef.current = false;
        activeSearchRunId.current = 0;
      }
    }
  }

  async function stopSearch() {
    const runId = activeSearchRunId.current;
    if (!runId) {
      return;
    }
    try {
      const accepted = await invoke<boolean>("stop_search", { runId });
      if (runId !== activeSearchRunId.current) {
        return;
      }
      setStatus(accepted ? "正在停止检索任务" : "停止请求尚未生效，请重试");
    } catch (error) {
      if (runId === activeSearchRunId.current) {
        setStatus(`停止失败: ${formatError(error)}`);
      }
    }
  }

  async function copyScript() {
    const scriptSnapshot = latestScriptRef.current;
    if (!scriptSnapshot || scriptSnapshot === "等待输入...") {
      return;
    }
    if (scriptValueTooLarge(scriptSnapshot)) {
      setStatus(`脚本内容过长，最多允许 ${MAX_SCRIPT_CHARS} 字节`);
      return;
    }
    try {
      const records = await invoke<HistoryRecord[]>("build_history_records", {
        script: scriptSnapshot,
      });
      const cleanRecords = sanitizeHistoryList(records);
      await writeText(scriptSnapshot);
      await enqueueHistorySave((currentHistory) => {
        const commands = new Set(cleanRecords.map((record) => record.command));
        return [
          ...cleanRecords,
          ...currentHistory.filter((record) => !commands.has(record.command)),
        ].slice(0, MAX_HISTORY_RECORDS);
      });
      setStatus(`已复制脚本并记录 ${cleanRecords.length} 条命令`);
    } catch (error) {
      setStatus(`复制失败: ${formatError(error)}`);
    }
  }

  async function pasteInput() {
    try {
      const value = await readText();
      if (value) {
        const result = acceptInputValue(value, "clipboard");
        if (result !== "rejected") {
          setStatus(
            result === "cleared"
              ? "已从剪贴板粘贴，旧检索结果和日志已清除"
              : "已从剪贴板粘贴",
          );
        }
      }
    } catch (error) {
      setStatus(`粘贴失败: ${formatError(error)}`);
    }
  }

  async function cleanComments() {
    const sourceScript = latestScriptRef.current;
    if (scriptValueTooLarge(sourceScript)) {
      setStatus(`脚本内容过长，最多允许 ${MAX_SCRIPT_CHARS} 字节`);
      return;
    }
    const requestSeq = scriptRequestSeq.current + 1;
    scriptRequestSeq.current = requestSeq;
    try {
      const cleaned = await invoke<string>("clean_script", { script: sourceScript });
      if (requestSeq !== scriptRequestSeq.current || sourceScript !== latestScriptRef.current) {
        return;
      }
      setScript(cleaned);
      setStatus("已移除脚本注释");
    } catch (error) {
      if (requestSeq === scriptRequestSeq.current && sourceScript === latestScriptRef.current) {
        setStatus(`清理失败: ${formatError(error)}`);
      }
    }
  }

  async function openSearchTabs() {
    if (browserOpenInProgress.current) {
      return;
    }
    if (inputTooLarge) {
      setStatus("输入超出限制，无法打开浏览器搜索");
      return;
    }
    const { names, total } = collectBrowserSearchNames(input, MAX_SEARCH_TABS);
    if (names.length === 0) {
      setStatus("没有可搜索的包名");
      return;
    }
    if (
      names.length > BROWSER_SEARCH_CONFIRM_THRESHOLD &&
      !window.confirm(
        total > names.length
          ? `检测到 ${total} 个可搜索包名，本次将按上限打开 ${names.length} 个浏览器页面，是否继续？`
          : `将要打开 ${names.length} 个浏览器页面，是否继续？`,
      )
    ) {
      setStatus("已取消浏览器搜索");
      return;
    }

    browserOpenInProgress.current = true;
    setOpeningSearchTabs(true);
    let opened = 0;
    let failed = 0;
    let lastError = "";
    try {
      for (let index = 0; index < names.length; index += 1) {
        try {
          await invoke("open_package_search", { packageName: names[index] });
          opened += 1;
        } catch (error) {
          failed += 1;
          lastError = formatError(error);
        }
        if (index + 1 < names.length) {
          await new Promise((resolve) => window.setTimeout(resolve, 180));
        }
      }
    } finally {
      browserOpenInProgress.current = false;
      setOpeningSearchTabs(false);
    }
    const details = [
      total > names.length ? `已按上限截断到 ${names.length} 个` : "",
      failed > 0 ? `${failed} 个失败${lastError ? `：${lastError}` : ""}` : "",
    ].filter(Boolean);
    setStatus(`已打开 ${opened} 个搜索页面${details.length > 0 ? `；${details.join("；")}` : ""}`);
  }

  async function persistSettings() {
    if (!beginSettingsOperation()) {
      return;
    }
    const actionSeq = settingsActionSeq.current + 1;
    settingsActionSeq.current = actionSeq;
    const settingsSnapshot = settings;
    try {
      const publicSettings = sanitizePublicSettings(
        await invoke<PublicSettings>("save_settings", { settings: settingsSnapshot }),
      );
      setTokenConfigured(publicSettings.githubTokenConfigured);
      if (actionSeq !== settingsActionSeq.current) {
        setStatus("设置已保存；检测到新的界面修改，请再次保存");
        return;
      }
      setSettings({
        proxy: publicSettings.proxy,
        githubToken: "",
        cranMirror: publicSettings.cranMirror,
        fullSearch: publicSettings.fullSearch,
      });
      setShowToken(false);
      setStatus("设置已保存并立即生效");
    } catch (error) {
      setStatus(
        actionSeq === settingsActionSeq.current
          ? `设置保存失败: ${formatError(error)}`
          : `先前设置保存失败，当前修改尚未保存: ${formatError(error)}`,
      );
    } finally {
      endSettingsOperation();
    }
  }

  async function clearSavedToken() {
    if (!beginSettingsOperation()) {
      return;
    }
    const actionSeq = settingsActionSeq.current + 1;
    settingsActionSeq.current = actionSeq;
    try {
      const publicSettings = sanitizePublicSettings(await invoke<PublicSettings>("clear_github_token"));
      setTokenConfigured(false);
      if (actionSeq !== settingsActionSeq.current) {
        setStatus("已清除保存的 GitHub Token；界面保留了新的修改");
        return;
      }
      setSettings((current) => ({
        ...current,
        proxy: publicSettings.proxy,
        githubToken: "",
        cranMirror: publicSettings.cranMirror,
        fullSearch: publicSettings.fullSearch,
      }));
      setShowToken(false);
      setStatus("已清除保存的 GitHub Token");
    } catch (error) {
      setStatus(
        actionSeq === settingsActionSeq.current
          ? `Token 清除失败: ${formatError(error)}`
          : `Token 清除失败，当前修改未受影响: ${formatError(error)}`,
      );
    } finally {
      endSettingsOperation();
    }
  }

  async function copyHistoryRecord(record: HistoryRecord) {
    const cleanRecord = sanitizeHistoryRecord(record);
    if (!cleanRecord.command) {
      setStatus("历史命令为空，无法复制");
      return;
    }
    try {
      await writeText(cleanRecord.command);
      setStatus(`已复制 ${cleanRecord.packageName || "历史命令"}`);
    } catch (error) {
      setStatus(`历史复制失败: ${formatError(error)}`);
    }
  }

  async function applyHistoryRecord(record: HistoryRecord) {
    const cleanRecord = sanitizeHistoryRecord(record);
    let valueToLoad = cleanRecord.packageName;

    // 如果是 install_url 类型，尝试从命令中提取 URL
    if (cleanRecord.command.includes("install_url(")) {
      const match = cleanRecord.command.match(/install_url\("([^"]+)"/);
      if (match && match[1]) {
        valueToLoad = match[1];
      }
    }

    const result = acceptInputValue(valueToLoad, "manual");
    if (result !== "rejected") {
      if (cleanRecord.toolName === "GitHub") {
        setMethod("github");
      } else if (cleanRecord.toolName === "Bioconductor") {
        setMethod("biocManager");
      } else if (cleanRecord.toolName === "remotes") {
        if (cleanRecord.command.includes("install_url")) {
          setMethod("remotes");
        } else {
          setMethod("auto");
        }
      } else if (cleanRecord.toolName === "devtools") {
        setMethod("devtools");
      } else if (cleanRecord.toolName === "base R") {
        if (cleanRecord.command.includes("packageVersion")) {
          setMethod("version");
        } else {
          setMethod("base");
        }
      }
      setView("workspace");
      setStatus(`已加载历史命令 ${cleanRecord.packageName} 至工作台`);
    }
  }

  async function deleteHistoryRecord(id: string) {
    try {
      await enqueueHistorySave((currentHistory) =>
        currentHistory.filter((record) => record.id !== id),
      );
      setStatus("历史记录已删除");
    } catch (error) {
      setStatus(`历史保存失败: ${formatError(error)}`);
    }
  }

  async function enqueueHistorySave(
    buildNext: (currentHistory: HistoryRecord[]) => HistoryRecord[],
  ) {
    historyActionSeq.current += 1;
    const task = historySaveQueue.current.then(async () => {
      const historyLoadReady = await waitForInitialHistoryLoad();
      if (!historyLoadReady) {
        setStatus("历史加载等待超时，已使用当前历史继续保存");
      }
      const nextHistory = sanitizeHistoryList(buildNext(latestHistoryRef.current));
      const savedHistory = await invoke<HistoryRecord[]>("save_history", { history: nextHistory });
      setHistory(savedHistory);
    });
    historySaveQueue.current = task.then(
      () => undefined,
      () => undefined,
    );
    await task;
  }

  async function waitForInitialHistoryLoad() {
    return Promise.race([
      historyLoadReadyRef.current?.then(() => true) ?? Promise.resolve(true),
      new Promise<boolean>((resolve) =>
        window.setTimeout(() => resolve(false), HISTORY_LOAD_WAIT_TIMEOUT_MS),
      ),
    ]);
  }

  function isMethodDisabled(candidate: Method) {
    return !methodSupportsInput(candidate, inputProfile);
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
          <progress
            className="summary-track"
            value={summaryProgress}
            max={100}
            aria-label="已验证包比例"
          />
          <small>
            {results.length ? `${foundCount} 条来源记录` : "等待开始"}
          </small>
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
          <div
            className={`status-chip ${searching ? "active" : ""}`}
            role="status"
            aria-live="polite"
            aria-atomic="true"
          >
            <i aria-hidden="true" />
            {status}
          </div>
        </header>

        <section className="content">
          {view === "workspace" && (
            <div className="workspace-grid">
              <section className="panel input-panel">
                <PanelHeader
                  step="01"
                  title="输入包列表"
                  meta={`${packageCount}/${MAX_PACKAGE_LINES} 项`}
                />
                <textarea
                  value={input}
                  onChange={(event) => acceptInputValue(event.currentTarget.value, "manual")}
                  placeholder={"每行一个包，例如：\nSeurat 5.2.1\nGSVA 1.50\nbuenrostrolab/FigR\nhttps://example.org/pkg_1.0.tar.gz"}
                  aria-label="R 包输入列表"
                  aria-describedby={inputTooLarge ? "input-limit-warning" : undefined}
                  aria-invalid={inputTooLarge}
                  spellCheck={false}
                  maxLength={MAX_INPUT_CHARS + 1}
                  disabled={searching}
                />
                {inputTooLarge && (
                  <div className="inline-warning" id="input-limit-warning" role="alert">
                    输入超出限制或包含非法字符：最多 {MAX_PACKAGE_LINES} 行、总计 {MAX_INPUT_CHARS} 字节、单行 {MAX_INPUT_LINE_BYTES} 字节。
                  </div>
                )}
                <div className="input-actions">
                  <button className="button ghost" onClick={pasteInput} disabled={searching}>粘贴</button>
                  <button
                    className="button ghost"
                    onClick={() => acceptInputValue("", "manual")}
                    disabled={searching}
                  >
                    清空
                  </button>
                  <button
                    className="button ghost wide"
                    onClick={openSearchTabs}
                    disabled={searching || openingSearchTabs || inputTooLarge}
                  >
                    {openingSearchTabs ? "正在打开..." : "浏览器搜索"}
                  </button>
                  {searching ? (
                    <button className="button danger" onClick={stopSearch}>停止</button>
                  ) : (
                    <button className="button primary" onClick={startSearch} disabled={!input.trim() || inputTooLarge}>
                      开始检索
                    </button>
                  )}
                </div>
              </section>

              <section className="panel method-panel">
                <PanelHeader step="02" title="安装策略" meta={settings.fullSearch ? "全量检索" : "快速检索"} />
                <div className="method-grid">
                  {methods.map((item) => (
                    <button
                      key={item.id}
                      className={`method-card ${method === item.id ? "selected" : ""}`}
                      disabled={isMethodDisabled(item.id)}
                      aria-pressed={method === item.id}
                      onClick={() => setMethod(item.id)}
                    >
                      <span>{item.title}</span>
                      <small>{item.description}</small>
                    </button>
                  ))}
                </div>
                <div className="toggle-row">
                  <Toggle
                    checked={conditional}
                    label="条件安装"
                    description="已安装时自动跳过"
                    onChange={setConditional}
                  />
                  <Toggle
                    checked={installDependencies}
                    label="安装依赖"
                    description="dependencies = TRUE"
                    onChange={setInstallDependencies}
                  />
                  <Toggle
                    checked={showRemoteVersion}
                    label="同步远程版本"
                    description="显示版本并生成精确版本安装"
                    onChange={setShowRemoteVersion}
                  />
                  <Toggle
                    checked={settings.fullSearch}
                    label="全量检索"
                    description="命中后仍继续查询 GitHub"
                    onChange={(value) =>
                      updateSettingsFromUser((current) => ({ ...current, fullSearch: value }))
                    }
                  />
                </div>
              </section>

              <section className="panel script-panel">
                <header className="panel-header" style={{ gridTemplateColumns: "auto auto 1fr auto" }}>
                  <span>03</span>
                  <h2>脚本预览</h2>
                  <div style={{ display: "flex", gap: "8px", justifyContent: "flex-end", marginRight: "10px" }}>
                    <button
                      className="button ghost"
                      style={{ padding: "4px 10px", fontSize: "11px", height: "30px", minHeight: "auto" }}
                      onClick={cleanComments}
                      disabled={scriptTooLarge}
                    >
                      移除注释
                    </button>
                    <button
                      className="button primary"
                      style={{ padding: "4px 12px", fontSize: "11px", height: "30px", minHeight: "auto" }}
                      onClick={copyScript}
                      disabled={!script || script === "等待输入..." || scriptTooLarge}
                    >
                      复制脚本
                    </button>
                  </div>
                  <small>R Script</small>
                </header>
                <pre aria-label="生成的 R 脚本" tabIndex={0}>{script}</pre>
                {scriptTooLarge && (
                  <div className="inline-warning">
                    脚本内容超出限制：最多 {MAX_SCRIPT_CHARS} 字节。
                  </div>
                )}
              </section>
            </div>
          )}

          {view === "report" && (
            <div className="report-layout">
              <div className="metric-row">
                <Metric label="输入包" value={packageCount} />
                <Metric label="已验证包" value={uniqueFoundCount} tone="success" />
                <Metric
                  label="未找到"
                  value={new Set(results.filter((item) => !item.found).map((item) => item.package)).size}
                  tone="danger"
                />
                <Metric label="来源记录" value={results.length} />
              </div>
              <section className="panel report-panel">
                <PanelHeader step="结果" title="来源验证" meta={searching ? "实时更新" : "已完成"} />
                {results.length === 0 ? (
                  <EmptyState text={searching ? "正在等待首条检索结果" : "尚未执行检索"} />
                ) : (
                  <div className="result-table" role="table" aria-label="包来源验证结果">
                    <div className="result-row result-head" role="row">
                      <span role="columnheader">包名</span>
                      <span role="columnheader">来源</span>
                      <span role="columnheader">版本</span>
                      <span role="columnheader">仓库</span>
                      <span role="columnheader">状态</span>
                    </div>
                    {results.map((result, index) => (
                      <div className="result-row" role="row" key={`${result.package}-${result.source}-${index}`}>
                        <strong role="cell">{result.package}</strong>
                        <span role="cell" className={`source-tag ${result.source}`}>{sourceNames[result.source] ?? result.source}</span>
                        <code role="cell">{result.latestVersion || "—"}</code>
                        <span role="cell" className="repo-cell">{result.repository || "—"}</span>
                        <span role="cell" className={result.found ? "found" : result.status === "timeout" ? "timeout" : result.status === "rateLimited" ? "rate-limited" : "missing"}>
                          {result.status === "timeout" ? "超时" : result.status === "rateLimited" ? "频率限制" : result.found ? "已验证" : "未找到"}
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </section>
              <section className="panel log-panel">
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <PanelHeader step="日志" title="检索过程" meta={`${logs.length} 行`} />
                  <button 
                    className="button ghost" 
                    style={{ marginRight: "16px", padding: "4px 8px", fontSize: "12px", height: "auto" }}
                    onClick={() => setLogs([])}
                    disabled={searching || logs.length === 0}
                  >
                    清除日志
                  </button>
                </div>
                <div className="log-console">
                  {logs.length ? logs.map((line, index) => <div key={`${line}-${index}`}><span>{String(index + 1).padStart(2, "0")}</span>{line}</div>) : <EmptyState text="日志将在检索开始后显示" />}
                </div>
              </section>
            </div>
          )}

          {view === "history" && (
            <section className="panel history-panel">
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                <PanelHeader step="历史" title="最近生成的命令" meta={`最多保留 100 条`} />
                <input
                  type="text"
                  placeholder="搜索包名、来源或命令..."
                  value={historySearch}
                  onChange={(e) => setHistorySearch(e.target.value)}
                  style={{ marginRight: "16px", padding: "4px 8px", fontSize: "12px", width: "200px" }}
                />
              </div>
              {history.length === 0 ? (
                <EmptyState text="复制脚本后，命令会记录在这里" />
              ) : (
                <div className="history-list">
                  {history
                    .filter(record => 
                      (record.packageName && record.packageName.toLowerCase().includes(historySearch.toLowerCase())) ||
                      (record.toolName && record.toolName.toLowerCase().includes(historySearch.toLowerCase())) ||
                      (record.command && record.command.toLowerCase().includes(historySearch.toLowerCase()))
                    )
                    .map((record) => (
                    <article className="history-item" key={record.id}>
                      <div className="history-main">
                        <div>
                          <strong>{record.packageName || "R 命令"}</strong>
                          <span>{record.toolName}{record.version ? ` · v${record.version}` : ""}</span>
                        </div>
                        <code>{record.command}</code>
                      </div>
                      <div className="history-actions">
                        <button className="text-button" onClick={() => applyHistoryRecord(record)}>应用</button>
                        <button className="text-button" onClick={() => copyHistoryRecord(record)}>复制</button>
                        <button className="text-button danger-text" onClick={() => deleteHistoryRecord(record.id)}>删除</button>
                      </div>
                    </article>
                  ))}
                </div>
              )}
            </section>
          )}

          {view === "settings" && (
            <div className="settings-layout">
              <section className="panel settings-panel">
                <PanelHeader step="网络" title="连接设置" meta="独立配置" />
                <label className="field">
                  <span>网络代理</span>
                  <small>支持 127.0.0.1:7890 或无凭据代理 URL，不允许路径或查询参数</small>
                  <input
                    value={settings.proxy}
                    onChange={(event) => acceptSettingValue("proxy", event.currentTarget.value)}
                    placeholder="不使用代理"
                    maxLength={MAX_RESULT_FIELD_CHARS}
                  />
                </label>
                <label className="field">
                  <span>GitHub Token</span>
                  <small>
                    {tokenConfigured
                      ? "已保存 Token；留空保存会继续保留现有 Token"
                      : "仅保存在本应用的数据目录，用于提高 API 配额"}
                  </small>
                  <div className="secret-field">
                    <input
                      type={showToken ? "text" : "password"}
                      value={settings.githubToken}
                      onChange={(event) => acceptSettingValue("githubToken", event.currentTarget.value)}
                      placeholder="ghp_..."
                      autoComplete="off"
                      spellCheck={false}
                      maxLength={MAX_TOKEN_CHARS}
                    />
                    <button type="button" onClick={() => setShowToken((value) => !value)}>
                      {showToken ? "隐藏" : "显示"}
                    </button>
                  </div>
                  {tokenConfigured && !settings.githubToken.trim() && (
                    <button
                      type="button"
                      className="text-button danger-text"
                      onClick={clearSavedToken}
                      disabled={settingsBusy}
                    >
                      清除已保存 Token
                    </button>
                  )}
                </label>
                <Toggle
                  checked={settings.fullSearch}
                  label="全量检索"
                  description="命中 CRAN 或 Bioconductor 后仍继续查询 GitHub"
                  onChange={(value) =>
                    updateSettingsFromUser((current) => ({ ...current, fullSearch: value }))
                  }
                />
                <div style={{ borderTop: "1px solid var(--line)", marginTop: "20px", paddingTop: "12px" }}>
                  <div className="field" style={{ margin: "0 17px" }}>
                    <span>界面风格</span>
                    <small>选择您偏好的系统色彩，切换实时生效</small>
                    <div className="theme-selector">
                      <button
                        type="button"
                        className={`theme-card ${currentTheme === "office" ? "selected" : ""}`}
                        onClick={() => handleThemeChange("office")}
                      >
                        <div className="theme-preview-dots">
                          <div className="theme-dot" style={{ background: "#0f172a" }} />
                          <div className="theme-dot" style={{ background: "#0f4c81" }} />
                          <div className="theme-dot" style={{ background: "#e6f0fa" }} />
                        </div>
                        <span>商务办公蓝</span>
                      </button>
                      <button
                        type="button"
                        className={`theme-card ${currentTheme === "green" ? "selected" : ""}`}
                        onClick={() => handleThemeChange("green")}
                      >
                        <div className="theme-preview-dots">
                          <div className="theme-dot" style={{ background: "#112c24" }} />
                          <div className="theme-dot" style={{ background: "#176b4d" }} />
                          <div className="theme-dot" style={{ background: "#dcece4" }} />
                        </div>
                        <span>墨绿林野</span>
                      </button>
                      <button
                        type="button"
                        className={`theme-card ${currentTheme === "graphite" ? "selected" : ""}`}
                        onClick={() => handleThemeChange("graphite")}
                      >
                        <div className="theme-preview-dots">
                          <div className="theme-dot" style={{ background: "#212529" }} />
                          <div className="theme-dot" style={{ background: "#495057" }} />
                          <div className="theme-dot" style={{ background: "#f1f3f5" }} />
                        </div>
                        <span>石墨暗灰</span>
                      </button>
                    </div>
                  </div>
                  <div className="field" style={{ margin: "0 17px", marginTop: "24px" }}>
                    <span>字体风格</span>
                    <small>选择最适合您显示器的排版</small>
                    <div className="theme-selector">
                      <button
                        type="button"
                        className={`theme-card ${currentFont === "modern" ? "selected" : ""}`}
                        onClick={() => handleFontChange("modern")}
                      >
                        <div className="theme-preview-dots" style={{ alignItems: 'center', justifyContent: 'center' }}>
                          <span style={{ fontFamily: "'Inter', 'Noto Sans SC', sans-serif", fontSize: '15px', fontWeight: 600, color: 'var(--ink)' }}>Aa</span>
                        </div>
                        <span>现代 (推荐)</span>
                      </button>
                      <button
                        type="button"
                        className={`theme-card ${currentFont === "system" ? "selected" : ""}`}
                        onClick={() => handleFontChange("system")}
                      >
                        <div className="theme-preview-dots" style={{ alignItems: 'center', justifyContent: 'center' }}>
                          <span style={{ fontFamily: '"Segoe UI", "Microsoft YaHei UI", sans-serif', fontSize: '15px', fontWeight: 600, color: 'var(--ink)' }}>Aa</span>
                        </div>
                        <span>系统默认</span>
                      </button>
                      <button
                        type="button"
                        className={`theme-card ${currentFont === "classic" ? "selected" : ""}`}
                        onClick={() => handleFontChange("classic")}
                      >
                        <div className="theme-preview-dots" style={{ alignItems: 'center', justifyContent: 'center' }}>
                          <span style={{ fontFamily: '"SimSun", "宋体", serif', fontSize: '15px', fontWeight: 600, color: 'var(--ink)' }}>Aa</span>
                        </div>
                        <span>传统宋体</span>
                      </button>
                    </div>
                  </div>
                </div>
              </section>

              <section className="panel settings-panel">
                <PanelHeader step="系统" title="应用更新" meta="版本维护" />
                <div className="field">
                  <span>检查应用更新</span>
                  <small>检查并安装最新版本的 R Package Command Center</small>
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center', marginTop: '9px' }}>
                    <button 
                      className="button primary" 
                      onClick={checkForUpdates} 
                      disabled={checkingUpdate}
                      style={{ marginLeft: 0 }}
                    >
                      {checkingUpdate ? '正在处理...' : '检查更新'}
                    </button>
                    {updateMessage && <span style={{fontSize: '14px', color: 'var(--muted)'}}>{updateMessage}</span>}
                  </div>
                </div>
              </section>

              <section className="panel settings-panel">
                <PanelHeader step="缓存" title="包结果缓存" meta="避免重复检索" />
                <div className="field">
                  <span>清除包缓存</span>
                  <small>已缓存的包将跳过在线检索直接使用历史结果；清除后所有包都会重新在线检索</small>
                  <div style={{ display: 'flex', gap: '8px', alignItems: 'center', marginTop: '9px' }}>
                    <button 
                      className="button ghost" 
                      onClick={async () => {
                        try {
                          await invoke("clear_package_cache");
                          setStatus("包缓存已清除");
                        } catch (error) {
                          setStatus(`缓存清除失败: ${formatError(error)}`);
                        }
                      }}
                      style={{ marginLeft: 0 }}
                    >
                      清除缓存
                    </button>
                    <button
                      className="button ghost"
                      onClick={async () => {
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
                        } catch (error) {
                          setStatus(`诊断导出失败: ${formatError(error)}`);
                        }
                      }}
                    >
                      导出诊断
                    </button>
                  </div>
                </div>
              </section>

              <section className="panel settings-panel">
                <PanelHeader step="镜像" title="CRAN 镜像" meta="实时影响脚本" />
                <div className="mirror-list">
                  {mirrors.map((mirror) => (
                    <button
                      key={mirror.value}
                      className={settings.cranMirror === mirror.value ? "selected" : ""}
                      aria-pressed={settings.cranMirror === mirror.value}
                      onClick={() =>
                        updateSettingsFromUser((current) => ({
                          ...current,
                          cranMirror: mirror.value,
                        }))
                      }
                    >
                      <span>{mirror.label}</span>
                      <code>{mirror.value}</code>
                    </button>
                  ))}
                </div>
                <label className="field compact">
                  <span>自定义镜像</span>
                  <input
                    value={settings.cranMirror}
                    onChange={(event) => acceptSettingValue("cranMirror", event.currentTarget.value)}
                    placeholder="https://cloud.r-project.org"
                    maxLength={MAX_RESULT_FIELD_CHARS}
                  />
                </label>
                <button
                  className="button primary save-button"
                  onClick={persistSettings}
                  disabled={settingsBusy}
                >
                  {settingsBusy ? "处理中..." : "保存设置"}
                </button>
              </section>
            </div>
          )}
        </section>
      </main>
    </div>
  );
}

export default App;
