import { renderHook, act } from '@testing-library/react';
import { useHistory } from './useHistory';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import * as tauriCore from '@tauri-apps/api/core';
import * as clipboard from '@tauri-apps/plugin-clipboard-manager';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
  writeText: vi.fn(),
}));

describe('useHistory Hook', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('在成功加载历史记录时应正确设置 history 状态', async () => {
    const mockHistory = [
      {
        id: '1',
        command: 'install.packages("ggplot2")',
        packageName: 'ggplot2',
        version: '3.4.0',
        toolName: 'devtools',
        createdAt: '2026-06-24T12:00:00Z',
      },
    ];

    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'load_history') return mockHistory;
      return null;
    });

    const setStatus = vi.fn();
    const { result } = renderHook(() => useHistory(setStatus));

    // 等待初始化 load_history 完成
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 10));
    });

    expect(result.current.history).toHaveLength(1);
    expect(result.current.history[0].packageName).toBe('ggplot2');
    expect(setStatus).not.toHaveBeenCalled();
  });

  it('在加载历史记录失败时应设置错误状态信息', async () => {
    vi.mocked(tauriCore.invoke).mockRejectedValue(new Error('磁盘读取失败'));

    const setStatus = vi.fn();
    renderHook(() => useHistory(setStatus));

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 10));
    });

    expect(setStatus).toHaveBeenCalledWith(expect.stringContaining('历史加载失败'));
  });

  it('应能够成功复制历史记录的命令到剪贴板', async () => {
    vi.mocked(tauriCore.invoke).mockResolvedValue([]);
    vi.mocked(clipboard.writeText).mockResolvedValue(undefined);

    const setStatus = vi.fn();
    const { result } = renderHook(() => useHistory(setStatus));

    const record = {
      id: '1',
      command: 'install.packages("ggplot2")',
      packageName: 'ggplot2',
      version: '3.4.0',
      toolName: 'devtools',
      createdAt: '2026-06-24T12:00:00Z',
    };

    await act(async () => {
      await result.current.copyHistoryRecord(record);
    });

    expect(clipboard.writeText).toHaveBeenCalledWith(record.command);
    expect(setStatus).toHaveBeenCalledWith(expect.stringContaining('已复制 ggplot2'));
  });

  it('在删除历史记录时应保存更新后的历史', async () => {
    const mockHistory = [
      {
        id: '1',
        command: 'install.packages("ggplot2")',
        packageName: 'ggplot2',
        version: '3.4.0',
        toolName: 'devtools',
        createdAt: '2026-06-24T12:00:00Z',
      },
    ];

    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'load_history') return mockHistory;
      if (cmd === 'save_history') {
        const next = (args as any).history;
        return next;
      }
      return null;
    });

    const setStatus = vi.fn();
    const { result } = renderHook(() => useHistory(setStatus));

    // 等待初始化
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 10));
    });

    await act(async () => {
      await result.current.deleteHistoryRecord('1');
    });

    expect(tauriCore.invoke).toHaveBeenCalledWith('save_history', { history: [] });
    expect(result.current.history).toHaveLength(0);
    expect(setStatus).toHaveBeenCalledWith('历史记录已删除');
  });
});
