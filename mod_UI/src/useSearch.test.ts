import { renderHook, act } from '@testing-library/react';
import { mergeSearchLogs, useSearch } from './useSearch';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import * as tauriCore from '@tauri-apps/api/core';
import * as tauriEvent from '@tauri-apps/api/event';
import { useScriptGeneration } from './useScriptGeneration';
import { defaultSettings } from './types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}));

describe('useSearch Hook', () => {
  it('空闲期间无周期定时器，事件突发仅安排一次刷新', async () => {
    vi.useFakeTimers();
    const status = vi.fn();
    let finish!: (value: unknown) => void;
    let runId = 0;
    vi.mocked(tauriCore.invoke).mockImplementation(async (_command, args) => {
      runId = (args as any).runId;
      return new Promise(resolve => { finish = resolve; });
    });
    const hook = renderHook(() => useSearch(status));
    try {
      expect(vi.getTimerCount()).toBe(0);
      let task!: Promise<void>;
      await act(async () => { task = hook.result.current.startSearch('a', defaultSettings, false, vi.fn(), vi.fn()); });
      act(() => {
        for (let i = 0; i < 20; i++) logCallback({ payload: { runId, messages: [`日志${i}`] } });
      });
      expect(vi.getTimerCount()).toBe(1);
      act(() => vi.advanceTimersByTime(32));
      expect(hook.result.current.logs).toHaveLength(20);
      expect(vi.getTimerCount()).toBe(0);
      await act(async () => { finish({ runId, results: [], logs: [], stopped: false }); await task; });
    } finally { hook.unmount(); vi.useRealTimers(); }
  });

  it('无关设置不重算脚本，库路径变化立即使旧脚本失效', async () => {
    vi.useFakeTimers();
    vi.mocked(tauriCore.invoke).mockResolvedValue('新脚本');
    const status = vi.fn();
    const results: any[] = [];
    const channels: string[] = [];
    const hook = renderHook(({ settings }) => useScriptGeneration(
      'dplyr', 'r', '', channels, 'auto', false, false, true, false, false,
      settings, '', results, false, status,
    ), { initialProps: { settings: defaultSettings } });
    try {
      await act(async () => vi.advanceTimersByTimeAsync(120));
      expect(tauriCore.invoke).toHaveBeenCalledTimes(1);
      hook.rerender({ settings: { ...defaultSettings, searchConcurrency: 8 } });
      expect(hook.result.current.latestScriptRef.current).toBe('新脚本');
      await act(async () => vi.advanceTimersByTimeAsync(500));
      expect(tauriCore.invoke).toHaveBeenCalledTimes(1);
      hook.rerender({ settings: { ...defaultSettings, rLibPath: 'D:/R/library' } });
      expect(hook.result.current.latestScriptRef.current).toBe('');
      await act(async () => vi.advanceTimersByTimeAsync(120));
      expect(tauriCore.invoke).toHaveBeenCalledTimes(2);
    } finally { hook.unmount(); vi.useRealTimers(); }
  });
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

  it('连点暂停按钮只提交一次请求且状态与后端一致', async () => {
    let finishSearch: () => void = () => {};
    const pauseCommands: string[] = [];
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'start_search') {
        await new Promise<void>((resolve) => { finishSearch = resolve; });
        return { runId: 1, results: [], logs: [], stopped: false };
      }
      if (cmd === 'pause_search' || cmd === 'resume_search') { pauseCommands.push(cmd); return true; }
      return null;
    });
    const setStatus = vi.fn();
    const { result } = renderHook(() => useSearch(setStatus));
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 10)); });
    let searchPromise: Promise<void> | undefined;
    act(() => { searchPromise = result.current.startSearch('ggplot2', {} as any, false, vi.fn(), vi.fn()); });
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });

    await act(async () => {
      await Promise.all([result.current.togglePauseSearch(), result.current.togglePauseSearch()]);
    });

    expect(pauseCommands).toEqual(['pause_search']);
    expect(result.current.paused).toBe(true);
    expect(setStatus).toHaveBeenLastCalledWith('检索已暂停');

    finishSearch();
    await act(async () => { await searchPromise; });
  });

  it('后端拒绝暂停请求时以后端为准复位本地状态', async () => {
    let finishSearch: () => void = () => {};
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'start_search') {
        await new Promise<void>((resolve) => { finishSearch = resolve; });
        return { runId: 1, results: [], logs: [], stopped: false };
      }
      if (cmd === 'pause_search') return false;
      return null;
    });
    const setStatus = vi.fn();
    const { result } = renderHook(() => useSearch(setStatus));
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 10)); });
    let searchPromise: Promise<void> | undefined;
    act(() => { searchPromise = result.current.startSearch('ggplot2', {} as any, false, vi.fn(), vi.fn()); });
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });

    await act(async () => { await result.current.togglePauseSearch(); });

    expect(result.current.paused).toBe(false);
    expect(setStatus).toHaveBeenLastCalledWith('检索任务已结束，无法切换暂停状态');

    finishSearch();
    await act(async () => { await searchPromise; });
  });
});
