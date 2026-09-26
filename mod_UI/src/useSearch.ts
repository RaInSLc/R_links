import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  asRecord, collectBrowserSearchNames, formatError,
  nextSearchRunId, safeRunId, safeStatusText, sanitizeSearchResponse,
  sanitizeSearchResult, resultIdentityKey,
  BROWSER_SEARCH_CONFIRM_THRESHOLD, MAX_SEARCH_LOGS, MAX_SEARCH_RESULTS, MAX_SEARCH_TABS,
  type SearchResponse, type SearchResult, type DependencyGraph, type SearchStageTiming,
} from "./utils";
import type { InputRules, Settings, SearchLogBatchEvent, SearchProgressEvent } from "./types";

type SetStatus = (s: string) => void;

function mergeSearchResults(current: SearchResult[], incoming: SearchResult[]) {
  const indexes = new Map(current.map((item, index) => [resultIdentityKey(item), index]));
  const next = [...current];
  for (const item of incoming) {
    const key = resultIdentityKey(item);
    const index = indexes.get(key);
    if (index !== undefined) next[index] = item;
    else if (next.length < MAX_SEARCH_RESULTS) { indexes.set(key, next.length); next.push(item); }
  }
  return next;
}

export function mergeSearchLogs(current: string[], incoming: string[]) {
  if (incoming.length === 0) return current;
  const tail = incoming.slice(current.length, MAX_SEARCH_LOGS);
  return tail.length ? [...current, ...tail].slice(0, MAX_SEARCH_LOGS) : current;
}

export function useSearch(setStatus: SetStatus) {
  const [results, setResults] = useState<SearchResult[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [dependencyGraph, setDependencyGraph] = useState<DependencyGraph | null>(null);
  const [searching, setSearching] = useState(false);
  const [paused, setPaused] = useState(false);
  const [openingSearchTabs, setOpeningSearchTabs] = useState(false);
  const [searchDuration, setSearchDuration] = useState<number | null>(null);
  const [stageTimings, setStageTimings] = useState<SearchStageTiming[]>([]);
  const activeSearchRunId = useRef(0);
  const searchingRef = useRef(false);
  const hasSearchEvidenceRef = useRef(false);
  const browserOpenInProgress = useRef(false);
  const searchStartTime = useRef(0);
  const listenerReadyRef = useRef<Promise<void>>(Promise.resolve());
  const flushEventsRef = useRef<() => void>(() => {});
  // 暂停状态以后端为准：本地 ref 作为即时真值，避免渲染闭包读到过期状态。
  const pausedRef = useRef(false);
  const pausePendingRef = useRef(false);
  const updatePaused = (value: boolean) => {
    pausedRef.current = value;
    setPaused(value);
  };

  useEffect(() => {
    let active = true;
    let pendingResults: SearchResult[] = [];
    let pendingLogs: string[] = [];
    let pendingRunId = 0;
    let timer: number | undefined;
    const flush = () => {
      window.clearTimeout(timer);
      timer = undefined;
      if (pendingRunId === activeSearchRunId.current && active) {
        const incomingResults = pendingResults;
        const incomingLogs = pendingLogs;
        if (incomingResults.length) setResults((current) => mergeSearchResults(current, incomingResults));
        if (incomingLogs.length) setLogs((current) => [...current, ...incomingLogs].slice(0, MAX_SEARCH_LOGS));
      }
      pendingResults = []; pendingLogs = [];
    };
    // 有事件才安排一次刷新，空闲时不再周期唤醒。
    const scheduleFlush = () => {
      if (timer === undefined) timer = window.setTimeout(flush, 32);
    };
    flushEventsRef.current = flush;
    const prepareBatch = (runId: number) => {
      if (pendingRunId !== runId) { pendingResults = []; pendingLogs = []; pendingRunId = runId; }
    };
    const unlistenLog = listen<SearchLogBatchEvent>(
      "search-log-batch",
      (event) => {
        const payload = asRecord(event.payload);
        if (!active || safeRunId(payload.runId) !== activeSearchRunId.current) return;
        hasSearchEvidenceRef.current = true;
        const messages = Array.isArray(payload.messages) ? payload.messages.map(m => safeStatusText(String(m))) : [];
        prepareBatch(activeSearchRunId.current);
        pendingLogs = [...pendingLogs, ...messages].slice(0, MAX_SEARCH_LOGS);
        if (pendingLogs.length) scheduleFlush();
      },
    ).catch((error) => {
      if (active) setStatus(`检索日志监听失败: ${formatError(error)}`);
      return () => undefined;
    });
    const unlistenProgress = listen<SearchProgressEvent>(
      "search-progress",
      (event) => {
        const payload = asRecord(event.payload);
        if (!active || safeRunId(payload.runId) !== activeSearchRunId.current) return;
        hasSearchEvidenceRef.current = true;
        prepareBatch(activeSearchRunId.current);
        const incoming = Array.isArray(payload.results) ? payload.results.slice(0, 32) : [payload.result];
        for (const result of incoming) if (pendingResults.length < MAX_SEARCH_RESULTS) pendingResults.push(sanitizeSearchResult(result));
        if (pendingResults.length) scheduleFlush();
      },
    ).catch((error) => {
      if (active) setStatus(`检索进度监听失败: ${formatError(error)}`);
      return () => undefined;
    });
    listenerReadyRef.current = Promise.all([unlistenLog, unlistenProgress]).then(() => undefined);
    return () => {
      active = false;
      window.clearTimeout(timer);
      void unlistenLog.then((u) => u());
      void unlistenProgress.then((u) => u());
    };
  }, [setStatus]);

  async function startSearch(
    input: string,
    settings: Settings,
    inputTooLarge: boolean,
    onViewReport: () => void,
    onSetMethodAuto: () => void,
  ) {
    if (!input.trim() || searchingRef.current || inputTooLarge) {
      if (inputTooLarge) {
        setStatus(`输入超出限制或包含非法字符`);
      }
      return;
    }
    const runId = nextSearchRunId();
    searchingRef.current = true;
    activeSearchRunId.current = runId;
    hasSearchEvidenceRef.current = false;
    try {
      await listenerReadyRef.current;
    } catch (error) {
      activeSearchRunId.current = 0;
      searchingRef.current = false;
      setStatus(`检索监听初始化失败: ${formatError(error)}`);
      return;
    }
    searchingRef.current = true;
    setSearching(true);
    updatePaused(false);
    setSearchDuration(null);
    setStageTimings([]);
    searchStartTime.current = Date.now();
    setResults([]);
    setLogs([]);
    setDependencyGraph(null);
    setStatus("正在检索包来源");
    onViewReport();
    try {
      const response = await invoke<SearchResponse>("start_search", { runId, input, settings });
      const clean = sanitizeSearchResponse(response);
      if (clean.runId !== activeSearchRunId.current) return;
      flushEventsRef.current();
      hasSearchEvidenceRef.current = clean.results.length > 0 || clean.logs.length > 0;
      setResults((current) => mergeSearchResults(current, clean.results));
      setLogs((current) => mergeSearchLogs(current, clean.logs));
      setDependencyGraph(clean.dependencyGraph || null);
      setStageTimings(clean.stageTimings || []);
      setStatus(clean.stopped ? "检索任务已停止" : "检索完成，脚本已自动刷新");
      if (!clean.stopped) onSetMethodAuto();
    } catch (error) {
      if (runId === activeSearchRunId.current) {
        setStatus(`检索失败: ${formatError(error)}`);
      }
    } finally {
      if (runId === activeSearchRunId.current) {
        flushEventsRef.current();
        const elapsed = Date.now() - searchStartTime.current;
        setSearchDuration(elapsed);
        setSearching(false);
        updatePaused(false);
        searchingRef.current = false;
        activeSearchRunId.current = 0;
      }
    }
  }

  async function startBinarySearch(
    input: string,
    settings: Settings,
    inputTooLarge: boolean,
    mirror: string,
    onViewReport: () => void,
  ) {
    if (!input.trim() || searchingRef.current || inputTooLarge) return;
    const runId = nextSearchRunId();
    searchingRef.current = true;
    activeSearchRunId.current = runId;
    hasSearchEvidenceRef.current = false;
    try { await listenerReadyRef.current; }
    catch (error) { activeSearchRunId.current = 0; searchingRef.current = false; setStatus(`检索监听初始化失败: ${formatError(error)}`); return; }
    searchingRef.current = true;
    setSearching(true); updatePaused(false); setSearchDuration(null); setResults([]); setLogs([]); setDependencyGraph(null); setStatus("正在检索 R 二进制包"); onViewReport();
    setStageTimings([]);
    searchStartTime.current = Date.now();
    try {
      const response = await invoke<SearchResponse>("start_binary_search", { runId, input, settings, mirror });
      const clean = sanitizeSearchResponse(response);
      if (clean.runId !== activeSearchRunId.current) return;
      hasSearchEvidenceRef.current = clean.results.length > 0 || clean.logs.length > 0;
      setResults(clean.results); setLogs(clean.logs); setDependencyGraph(null); setStatus(clean.stopped ? "检索任务已停止" : "R 二进制包检索完成");
    } catch (error) {
      if (runId === activeSearchRunId.current) setStatus(`R 二进制包检索失败: ${formatError(error)}`);
    } finally {
      if (runId === activeSearchRunId.current) {
      setSearchDuration(Date.now() - searchStartTime.current);
      setSearching(false);
      updatePaused(false);
      searchingRef.current = false;
      activeSearchRunId.current = 0;
    }
    }
  }

  async function startMultiEcosystemSearch(
    input: string,
    ecosystem: "pip" | "conda",
    settings: Settings,
    inputTooLarge: boolean,
    onViewReport: () => void,
  ) {
    if (!input.trim() || searchingRef.current || inputTooLarge) return;
    const runId = nextSearchRunId();
    searchingRef.current = true;
    activeSearchRunId.current = runId;
    hasSearchEvidenceRef.current = false;
    try {
      await listenerReadyRef.current;
    } catch (error) {
      activeSearchRunId.current = 0;
      searchingRef.current = false;
      setStatus(`检索监听初始化失败: ${formatError(error)}`);
      return;
    }
    searchingRef.current = true;
    setSearching(true);
    updatePaused(false);
    setSearchDuration(null);
    setStageTimings([]);
    setResults([]);
    setLogs([]);
    setDependencyGraph(null);
    setStatus(ecosystem === "pip" ? "正在检索 Python 包" : "正在检索 Conda 包");
    onViewReport();
    searchStartTime.current = Date.now();
    try {
      const response = await invoke<SearchResponse>("search_multi_ecosystem", { runId, input, ecosystem, settings });
      const clean = sanitizeSearchResponse(response);
      if (clean.runId !== activeSearchRunId.current) return;
      setResults(clean.results);
      hasSearchEvidenceRef.current = clean.results.length > 0 || clean.logs.length > 0;
      setLogs(clean.logs);
      setDependencyGraph(null);
      setStatus(clean.stopped ? "检索任务已停止" : (ecosystem === "pip" ? "Python 包检索完成" : "Conda 包检索完成"));
    } catch (error) {
      if (runId === activeSearchRunId.current) {
        setStatus(`${ecosystem === "pip" ? "Python" : "Conda"} 包检索失败: ${formatError(error)}`);
      }
    } finally {
      if (runId === activeSearchRunId.current) {
        setSearchDuration(Date.now() - searchStartTime.current);
        setSearching(false);
        updatePaused(false);
        searchingRef.current = false;
        activeSearchRunId.current = 0;
      }
    }
  }

  async function stopSearch() {
    const runId = activeSearchRunId.current;
    if (!runId) return;
    try {
      const accepted = await invoke<boolean>("stop_search", { runId });
      if (runId !== activeSearchRunId.current) return;
      setStatus(accepted ? "正在停止检索任务" : "停止请求尚未生效，请重试");
    } catch (error) {
      if (runId === activeSearchRunId.current) {
        setStatus(`停止失败: ${formatError(error)}`);
      }
    }
  }

  async function togglePauseSearch() {
    const runId = activeSearchRunId.current;
    // pending 期间忽略重复点击，避免连点造成前后端暂停状态漂移。
    if (!runId || pausePendingRef.current) return;
    pausePendingRef.current = true;
    const nextPaused = !pausedRef.current;
    try {
      const accepted = await invoke<boolean>(nextPaused ? "pause_search" : "resume_search", { runId });
      if (runId !== activeSearchRunId.current) return;
      if (accepted) {
        updatePaused(nextPaused);
        setStatus(nextPaused ? "检索已暂停" : "检索已继续");
      } else {
        // 后端拒绝（任务已结束或 runId 不匹配）：以后端状态为准复位本地暂停标记。
        updatePaused(false);
        setStatus("检索任务已结束，无法切换暂停状态");
      }
    } catch (error) {
      setStatus(`检索任务控制失败: ${formatError(error)}`);
    } finally {
      pausePendingRef.current = false;
    }
  }

  async function cancelSearchPackage(packageName: string) {
    const runId = activeSearchRunId.current;
    if (!runId || !packageName.trim()) return false;
    try {
      const accepted = await invoke<boolean>("cancel_search_package", {
        runId,
        package: packageName,
      });
      if (runId !== activeSearchRunId.current) return false;
      if (accepted) setStatus(`已取消包 ${packageName} 的结果提交；在途请求结束后丢弃结果`);
      return accepted;
    } catch (error) {
      setStatus(`取消包检索失败: ${formatError(error)}`);
      return false;
    }
  }

  async function openSearchTabs(input: string, inputTooLarge: boolean, ecosystem?: string, rules?: InputRules) {
    if (browserOpenInProgress.current) return;
    if (inputTooLarge) {
      setStatus("输入超出限制，无法打开浏览器搜索");
      return;
    }
    const { names, total } = collectBrowserSearchNames(input, MAX_SEARCH_TABS, rules);
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
      for (let i = 0; i < names.length; i += 1) {
        try {
          await invoke("open_package_search", { packageName: names[i], ecosystem: ecosystem || "r" });
          opened += 1;
        } catch (error) {
          failed += 1;
          lastError = formatError(error);
        }
        if (i + 1 < names.length) {
          await new Promise((r) => window.setTimeout(r, 180));
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

  return {
    results, setResults,
    logs, setLogs,
    dependencyGraph, setDependencyGraph,
    searching, setSearching,
    paused,
    openingSearchTabs,
    searchingRef,
    hasSearchEvidenceRef,
    searchDuration, stageTimings,
    startSearch,
    startBinarySearch,
    startMultiEcosystemSearch,
    stopSearch,
    togglePauseSearch,
    cancelSearchPackage,
    openSearchTabs,
  };
}
