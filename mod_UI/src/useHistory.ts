import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  asArray, formatError, mapBounded, sanitizeHistoryRecord,
  MAX_HISTORY_RECORDS, HISTORY_LOAD_WAIT_TIMEOUT_MS,
  type HistoryRecord,
} from "./utils";

type SetStatus = (s: string) => void;

export function useHistory(setStatus: SetStatus) {
  const [history, setHistoryState] = useState<HistoryRecord[]>([]);
  const [historySearch, setHistorySearch] = useState("");
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

  useEffect(() => {
    let active = true;
    invoke<HistoryRecord[]>("load_history")
      .then((nextHistory) => {
        if (active) loadInitialHistory(nextHistory);
      })
      .catch((error) => {
        if (active) setStatus(`历史加载失败: ${formatError(error)}`);
      })
      .finally(() => historyLoadResolveRef.current());
    return () => { active = false; };
  }, []);

  function sanitizeHistoryList(nextHistory: unknown): HistoryRecord[] {
    return mapBounded(asArray(nextHistory), MAX_HISTORY_RECORDS, sanitizeHistoryRecord);
  }

  function setHistory(nextHistory: unknown) {
    const clean = sanitizeHistoryList(nextHistory);
    latestHistoryRef.current = clean;
    setHistoryState(clean);
  }

  function loadInitialHistory(nextHistory: unknown) {
    const clean = sanitizeHistoryList(nextHistory);
    latestHistoryRef.current = clean;
    if (historyActionSeq.current === 0) {
      setHistoryState(clean);
    }
  }

  async function waitForInitialHistoryLoad() {
    return Promise.race([
      historyLoadReadyRef.current?.then(() => true) ?? Promise.resolve(true),
      new Promise<boolean>((resolve) =>
        window.setTimeout(() => resolve(false), HISTORY_LOAD_WAIT_TIMEOUT_MS),
      ),
    ]);
  }

  async function enqueueHistorySave(
    buildNext: (current: HistoryRecord[]) => HistoryRecord[],
  ) {
    historyActionSeq.current += 1;
    const task = historySaveQueue.current.then(async () => {
      const ready = await waitForInitialHistoryLoad();
      if (!ready) {
        setStatus("历史加载等待超时，已使用当前历史继续保存");
      }
      const next = sanitizeHistoryList(buildNext(latestHistoryRef.current));
      const saved = await invoke<HistoryRecord[]>("save_history", { history: next });
      setHistory(saved);
    });
    historySaveQueue.current = task.then(() => undefined, () => undefined);
    await task;
  }

  async function copyHistoryRecord(record: HistoryRecord) {
    const clean = sanitizeHistoryRecord(record);
    if (!clean.command) {
      setStatus("历史命令为空，无法复制");
      return;
    }
    try {
      await writeText(clean.command);
      setStatus(`已复制 ${clean.packageName || "历史命令"}`);
    } catch (error) {
      setStatus(`历史复制失败: ${formatError(error)}`);
    }
  }

  async function deleteHistoryRecord(id: string) {
    try {
      await enqueueHistorySave((current) =>
        current.filter((r) => r.id !== id),
      );
      setStatus("历史记录已删除");
    } catch (error) {
      setStatus(`历史保存失败: ${formatError(error)}`);
    }
  }

  async function clearAllHistory() {
    try {
      await enqueueHistorySave(() => []);
      setStatus("所有历史记录已清空");
    } catch (error) {
      setStatus(`清空历史失败: ${formatError(error)}`);
    }
  }

  return {
    history, historySearch, setHistorySearch,
    sanitizeHistoryList,
    enqueueHistorySave,
    copyHistoryRecord,
    deleteHistoryRecord,
    clearAllHistory,
  };
}