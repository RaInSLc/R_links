import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  appendBounded, asRecord, collectBrowserSearchNames, formatError,
  nextSearchRunId, safeRunId, safeStatusText, sanitizeSearchResponse,
  sanitizeSearchResult, upsertBoundedResult,
  BROWSER_SEARCH_CONFIRM_THRESHOLD, MAX_SEARCH_LOGS, MAX_SEARCH_RESULTS, MAX_SEARCH_TABS,
  type SearchResponse, type SearchResult, type DependencyGraph,
} from "./utils";
import type { Settings, SearchLogBatchEvent, SearchProgressEvent } from "./types";

type SetStatus = (s: string) => void;

function mergeSearchResults(current: SearchResult[], incoming: SearchResult[]) {
  let next = current;
  for (const item of incoming) {
    next = upsertBoundedResult(next, item, MAX_SEARCH_RESULTS);
  }
  return next;
}

export function useSearch(setStatus: SetStatus) {
  const [results, setResults] = useState<SearchResult[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [dependencyGraph, setDependencyGraph] = useState<DependencyGraph | null>(null);
  const [searching, setSearching] = useState(false);
  const [openingSearchTabs, setOpeningSearchTabs] = useState(false);
  const [searchDuration, setSearchDuration] = useState<number | null>(null);
  const activeSearchRunId = useRef(0);
  const searchingRef = useRef(false);
  const hasSearchEvidenceRef = useRef(false);
  const browserOpenInProgress = useRef(false);
  const searchStartTime = useRef(0);

  useEffect(() => {
    let active = true;
    const unlistenLog = listen<SearchLogBatchEvent>(
      "search-log-batch",
      (event) => {
        const payload = asRecord(event.payload);
        if (!active || safeRunId(payload.runId) !== activeSearchRunId.current) return;
        hasSearchEvidenceRef.current = true;
        const messages = Array.isArray(payload.messages) ? payload.messages.map(m => safeStatusText(String(m))) : [];
        setLogs((current) => {
            let next = [...current];
            for (const msg of messages) {
                next = appendBounded(next, msg, MAX_SEARCH_LOGS);
            }
            return next;
        });
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
        setResults((current) =>
          upsertBoundedResult(current, sanitizeSearchResult(payload.result), MAX_SEARCH_RESULTS),
        );
      },
    ).catch((error) => {
      if (active) setStatus(`检索进度监听失败: ${formatError(error)}`);
      return () => undefined;
    });
    return () => {
      active = false;
      void unlistenLog.then((u) => u());
      void unlistenProgress.then((u) => u());
    };
  }, []);

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
    searchingRef.current = true;
    setSearching(true);
    setSearchDuration(null);
    searchStartTime.current = Date.now();
    const runId = nextSearchRunId();
    activeSearchRunId.current = runId;
    hasSearchEvidenceRef.current = false;
    setResults([]);
    setLogs([]);
    setDependencyGraph(null);
    setStatus("正在检索包来源");
    onViewReport();
    try {
      const response = await invoke<SearchResponse>("start_search", { runId, input, settings });
      const clean = sanitizeSearchResponse(response);
      if (clean.runId !== activeSearchRunId.current) return;
      hasSearchEvidenceRef.current = clean.results.length > 0 || clean.logs.length > 0;
      setResults((current) => mergeSearchResults(current, clean.results));
      setLogs((current) => {
        let next = current;
        for (const msg of clean.logs) {
          next = appendBounded(next, msg, MAX_SEARCH_LOGS);
        }
        return next;
      });
      setDependencyGraph(clean.dependencyGraph || null);
      setStatus(clean.stopped ? "检索任务已停止" : "检索完成，脚本已自动刷新");
      if (!clean.stopped) onSetMethodAuto();
    } catch (error) {
      if (runId === activeSearchRunId.current) {
        setStatus(`检索失败: ${formatError(error)}`);
      }
    } finally {
      if (runId === activeSearchRunId.current) {
        const elapsed = Date.now() - searchStartTime.current;
        setSearchDuration(elapsed);
        setSearching(false);
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

  async function openSearchTabs(input: string, inputTooLarge: boolean) {
    if (browserOpenInProgress.current) return;
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
      for (let i = 0; i < names.length; i += 1) {
        try {
          await invoke("open_package_search", { packageName: names[i] });
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
    openingSearchTabs,
    searchingRef,
    hasSearchEvidenceRef,
    searchDuration,
    startSearch,
    stopSearch,
    openSearchTabs,
  };
}
