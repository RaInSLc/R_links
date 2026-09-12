import { renderHook, act } from '@testing-library/react';
import { mergeSearchLogs, useSearch } from './useSearch';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import * as tauriCore from '@tauri-apps/api/core';
import * as tauriEvent from '@tauri-apps/api/event';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}));

describe('useSearch Hook', () => {
  it('兼容批量进度并忽略其他任务的结果', async () => {
    let progress: (event: any) => void = () => {};
    let finish: () => void = () => {};
    let runId = 0;
    vi.mocked(tauriEvent.listen).mockImplementation(async (name, callback) => { if (name === 'search-progress') progress = callback; return () => {}; });
    vi.mocked(tauriCore.invoke).mockImplementation(async (_name, args) => { runId = (args as any).runId; await new Promise<void>((resolve) => { finish = resolve; }); return { runId, results: [], logs: [], stopped: false }; });
    const status = vi.fn();
    const { result } = renderHook(() => useSearch(status));
    let task: Promise<void>;
    await act(async () => { task = result.current.startSearch('a\nb', {} as any, false, vi.fn(), vi.fn()); await Promise.resolve(); });
    await act(async () => { progress({ payload: { runId, results: [{ package: 'a' }, { package: 'b' }] } }); progress({ payload: { runId: runId + 1, results: [{ package: 'wrong' }] } }); finish(); await task!; });
    expect(result.current.results.map((item) => item.package)).toEqual(['a', 'b']);
  });
  it('二进制检索停止响应不得显示成功完成', async () => {
    vi.mocked(tauriCore.invoke).mockImplementation(async (_command, args) => ({ runId: (args as { runId: number }).runId, results: [], logs: [], stopped: true }));
    const setStatus = vi.fn();
    const { result } = renderHook(() => useSearch(setStatus));
    await act(async () => { await result.current.startBinarySearch('pkg', {} as any, false, 'https://cloud.r-project.org', vi.fn()); });
    expect(setStatus).toHaveBeenLastCalledWith('检索任务已停止');
    expect(result.current.searching).toBe(false);
  });
  let logCallback: (event: any) => void;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriEvent.listen).mockImplementation(async (event, callback) => {
      if (event === 'search-log-batch') {
        logCallback = callback;
      }
      return () => undefined;
    });
  });

  it('合并最终响应日志时不重复追加已流式显示的日志', () => {
    expect(mergeSearchLogs(['开始', 'CRAN 命中'], ['开始', 'CRAN 命中'])).toEqual(['开始', 'CRAN 命中']);
  });

  it('合并最终响应日志时会补齐未通过事件到达的尾部日志', () => {
    expect(mergeSearchLogs(['开始'], ['开始', 'CRAN 命中', '检索完成'])).toEqual(['开始', 'CRAN 命中', '检索完成']);
  });

  it('在收到具有匹配 runId 的 search-log-batch 事件时，应收集并拼接日志消息', async () => {
    let lastRunId = 0;
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'start_search') {
        lastRunId = (args as any).runId;
        await new Promise((resolve) => setTimeout(resolve, 50));
        return {
          runId: lastRunId,
          results: [],
          logs: [],
          stopped: false,
        };
      }
      return null;
    });

    const setStatus = vi.fn();
    const { result } = renderHook(() => useSearch(setStatus));

    // 等待 useEffect 中的 listen 异步注册完成！
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 10));
    });

    let searchPromise: Promise<void> | undefined;
    act(() => {
      searchPromise = result.current.startSearch(
        'ggplot2',
        { fullSearch: false, proxy: '', cranMirror: '', githubToken: '' } as any,
        false,
        vi.fn(),
        vi.fn()
      );
    });

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 10));
    });

    // 触发一个匹配当前 runId 的日志消息
    await act(async () => {
      logCallback({
        payload: {
          runId: lastRunId,
          messages: ['测试日志信息 1', '测试日志信息 2'],
        },
      });
      await new Promise((resolve) => setTimeout(resolve, 35));
    });

    expect(result.current.logs).toContain('测试日志信息 1');
    expect(result.current.logs).toContain('测试日志信息 2');

    await act(async () => {
      if (searchPromise) await searchPromise;
    });
  });

  it('启动检索前应等待日志和进度监听注册完成', async () => {
    const resolveListeners: Array<() => void> = [];
    vi.mocked(tauriEvent.listen).mockImplementation(async (event, callback) => {
      if (event === 'search-log-batch') logCallback = callback;
      await new Promise<void>((resolve) => { resolveListeners.push(resolve); });
      return () => undefined;
    });
    vi.mocked(tauriCore.invoke).mockResolvedValue({ runId: 1, results: [], logs: [], stopped: false } as any);
    const { result } = renderHook(() => useSearch(vi.fn()));
    let searchPromise: Promise<void> | undefined;
    act(() => {
      searchPromise = result.current.startSearch('ggplot2', {} as any, false, vi.fn(), vi.fn());
      void result.current.startSearch('other', {} as any, false, vi.fn(), vi.fn());
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(tauriCore.invoke).not.toHaveBeenCalledWith('start_search', expect.anything());
    resolveListeners.forEach((resolve) => resolve());
    await act(async () => { await searchPromise; });
    expect(tauriCore.invoke).toHaveBeenCalledWith('start_search', expect.anything());
    expect(vi.mocked(tauriCore.invoke).mock.calls.filter(([command]) => command === 'start_search')).toHaveLength(1);
  });

  it('在收到不匹配的 runId 时，应过滤并忽略该日志', async () => {
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'start_search') {
        await new Promise((resolve) => setTimeout(resolve, 50));
        return {
          runId: (args as any).runId,
          results: [],
          logs: [],
          stopped: false,
        };
      }
      return null;
    });

    const setStatus = vi.fn();
    const { result } = renderHook(() => useSearch(setStatus));

    // 等待 useEffect 中的 listen 异步注册完成！
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 10));
    });

    let searchPromise: Promise<void> | undefined;
    act(() => {
      searchPromise = result.current.startSearch(
        'ggplot2',
        { fullSearch: false, proxy: '', cranMirror: '', githubToken: '' } as any,
        false,
        vi.fn(),
        vi.fn()
      );
    });

    await act(async () => {
      logCallback({
        payload: {
          runId: 999, // 故意传入不匹配的 runId
          messages: ['异常日志'],
        },
      });
    });

    await act(async () => {
      if (searchPromise) await searchPromise;
    });

    expect(result.current.logs).not.toContain('异常日志');
  });

  it('可以暂停并继续当前检索任务', async () => {
    let finishSearch: () => void = () => {};
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'start_search') {
        await new Promise<void>((resolve) => { finishSearch = resolve; });
        return { runId: 1, results: [], logs: [], stopped: false };
      }
      if (cmd === 'pause_search' || cmd === 'resume_search') return true;
      return null;
    });
    const setStatus = vi.fn();
    const { result } = renderHook(() => useSearch(setStatus));
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 10)); });
    let searchPromise: Promise<void> | undefined;
    act(() => { searchPromise = result.current.startSearch('ggplot2', {} as any, false, vi.fn(), vi.fn()); });
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
    await act(async () => { await result.current.togglePauseSearch(); });
    expect(result.current.paused).toBe(true);
    await act(async () => { await result.current.togglePauseSearch(); });
    expect(result.current.paused).toBe(false);
    finishSearch();
    await act(async () => { await searchPromise; });
  });
});
