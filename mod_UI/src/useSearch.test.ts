import { renderHook, act } from '@testing-library/react';
import { useSearch } from './useSearch';
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
  let logCallback: (event: any) => void;
  let progressCallback: (event: any) => void;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriEvent.listen).mockImplementation(async (event, callback) => {
      if (event === 'search-log-batch') {
        logCallback = callback;
      } else if (event === 'search-progress') {
        progressCallback = callback;
      }
      return () => undefined;
    });
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

    let searchPromise;
    act(() => {
      searchPromise = result.current.startSearch(
        'ggplot2',
        { fullSearch: false, proxy: '', cranMirror: '', githubToken: '' } as any,
        false,
        vi.fn(),
        vi.fn()
      );
    });

    // 触发一个匹配当前 runId 的日志消息
    await act(async () => {
      logCallback({
        payload: {
          runId: lastRunId,
          messages: ['测试日志信息 1', '测试日志信息 2'],
        },
      });
    });

    expect(result.current.logs).toContain('测试日志信息 1');
    expect(result.current.logs).toContain('测试日志信息 2');

    await act(async () => {
      await searchPromise;
    });
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

    let searchPromise;
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
      await searchPromise;
    });

    expect(result.current.logs).not.toContain('异常日志');
  });
});
