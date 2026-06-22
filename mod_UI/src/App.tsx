import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  readText,
  writeText,
} from "@tauri-apps/plugin-clipboard-manager";
import "./App.css";
import { NavButton } from "./components";
import { WorkspaceView } from "./WorkspaceView";
import { ReportView } from "./ReportView";
import { HistoryView } from "./HistoryView";
import { SettingsView } from "./SettingsView";
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
  defaultSettings,
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
            <WorkspaceView
              input={input}
              inputTooLarge={inputTooLarge}
              inputProfile={inputProfile}
              method={method}
              conditional={conditional}
              installDependencies={installDependencies}
              showRemoteVersion={showRemoteVersion}
              settings={settings}
              script={script}
              scriptTooLarge={scriptTooLarge}
              searching={searching}
              openingSearchTabs={openingSearchTabs}
              onInputChange={acceptInputValue}
              onPaste={pasteInput}
              onClear={() => acceptInputValue("", "manual")}
              onOpenSearchTabs={openSearchTabs}
              onStartSearch={startSearch}
              onStopSearch={stopSearch}
              onMethodChange={setMethod}
              onConditionalChange={setConditional}
              onInstallDependenciesChange={setInstallDependencies}
              onShowRemoteVersionChange={setShowRemoteVersion}
              onFullSearchChange={(v) => updateSettingsFromUser((c) => ({ ...c, fullSearch: v }))}
              onCopyScript={copyScript}
              onCleanComments={cleanComments}
              isMethodDisabled={isMethodDisabled}
            />
          )}

          {view === "report" && (
            <ReportView
              results={results}
              logs={logs}
              packageCount={packageCount}
              uniqueFoundCount={uniqueFoundCount}
              searching={searching}
              onClearLogs={() => setLogs([])}
            />
          )}

          {view === "history" && (
            <HistoryView
              history={history}
              historySearch={historySearch}
              onHistorySearchChange={setHistorySearch}
              onApplyRecord={applyHistoryRecord}
              onCopyRecord={copyHistoryRecord}
              onDeleteRecord={deleteHistoryRecord}
            />
          )}

          {view === "settings" && (
            <SettingsView
              settings={settings}
              tokenConfigured={tokenConfigured}
              showToken={showToken}
              settingsBusy={settingsBusy}
              currentTheme={currentTheme}
              currentFont={currentFont}
              checkingUpdate={checkingUpdate}
              updateMessage={updateMessage}
              onProxyChange={(v) => acceptSettingValue("proxy", v)}
              onTokenChange={(v) => acceptSettingValue("githubToken", v)}
              onTokenToggle={() => setShowToken((v) => !v)}
              onClearToken={clearSavedToken}
              onFullSearchChange={(v) => updateSettingsFromUser((c) => ({ ...c, fullSearch: v }))}
              onCranMirrorChange={(v) => acceptSettingValue("cranMirror", v)}
              onMirrorSelect={(v) => updateSettingsFromUser((c) => ({ ...c, cranMirror: v }))}
              onSaveSettings={persistSettings}
              onThemeChange={handleThemeChange}
              onFontChange={handleFontChange}
              onCheckUpdates={checkForUpdates}
              onClearCache={async () => {
                try {
                  await invoke("clear_package_cache");
                  setStatus("包缓存已清除");
                } catch (error) {
                  setStatus(`缓存清除失败: ${formatError(error)}`);
                }
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
                } catch (error) {
                  setStatus(`诊断导出失败: ${formatError(error)}`);
                }
              }}
            />
          )}
        </section>
      </main>
    </div>
  );
}

export default App;
