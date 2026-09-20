import { renderHook, act } from '@testing-library/react';
import { useSettings } from './useSettings';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import * as tauriCore from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('useSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should format error on load failure', async () => {
    vi.mocked(tauriCore.invoke).mockRejectedValue(new Error('Failed to load'));

    const setStatus = vi.fn();
    renderHook(() => useSettings(setStatus));

    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 10));
    });

    expect(setStatus).toHaveBeenCalledWith(expect.stringContaining('设置加载失败: Failed to load'));
  });

  it('should save settings', async () => {
    const mockSettings = { fullSearch: true, searchConcurrency: 6, archiveGithubMajorGap: 1, proxy: '127.0.0.1:8080', cranMirror: '', githubTokenConfigured: false, githubToken: '', useCache: true, maxCacheEntries: 1000, useFilter: true, resolveDependencies: true, maxDependencyDepth: 2, includeLightDependencies: false, maxDependencyNodes: 100, pinnedMethods: ['auto', 'base', 'biocManager', 'github'] };
    
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'save_settings') return { ...mockSettings, githubToken: undefined };
      return null;
    });

    const setStatus = vi.fn();
    const { result } = renderHook(() => useSettings(setStatus));

    await act(async () => {
      await result.current.persistSettings(mockSettings as any);
    });

    expect(tauriCore.invoke).toHaveBeenCalledWith('save_settings', { settings: expect.anything() });
    expect(setStatus).toHaveBeenCalledWith('设置已保存并立即生效');
  });

  it('should save the latest settings snapshot after immediate user updates', async () => {
    const saved = { proxy: 'http://127.0.0.1:7890', githubTokenConfigured: false, cranMirror: 'https://cloud.r-project.org/', fullSearch: true, searchConcurrency: 6, archiveGithubMajorGap: 1, conditional: true, installDependencies: true, showRemoteVersion: true, useCache: true, maxCacheEntries: 1000, useFilter: true, resolveDependencies: false, maxDependencyDepth: 2, includeLightDependencies: true, maxDependencyNodes: 100, pinnedMethods: ['auto', 'base', 'biocManager', 'github'] };
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'load_settings') return { ...saved, proxy: '', fullSearch: false, resolveDependencies: true, includeLightDependencies: false };
      if (cmd === 'save_settings') return saved;
      return null;
    });

    const setStatus = vi.fn();
    const { result } = renderHook(() => useSettings(setStatus));

    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 10));
    });
    act(() => {
      result.current.updateSettingsFromUser((current) => ({ ...current, proxy: 'http://127.0.0.1:7890', resolveDependencies: false }));
    });
    await act(async () => {
      await result.current.persistSettings({ includeLightDependencies: true });
    });

    expect(tauriCore.invoke).toHaveBeenCalledWith('save_settings', {
      settings: expect.objectContaining({
        proxy: 'http://127.0.0.1:7890',
        resolveDependencies: false,
        includeLightDependencies: true,
      }),
    });
  });

  it('should refresh dependency settings after clearing saved token', async () => {
    const publicSettings = { proxy: '', githubTokenConfigured: false, cranMirror: 'https://cloud.r-project.org/', fullSearch: false, searchConcurrency: 8, archiveGithubMajorGap: 2, conditional: true, installDependencies: true, showRemoteVersion: true, useCache: true, maxCacheEntries: 1000, useFilter: true, resolveDependencies: false, maxDependencyDepth: 4, includeLightDependencies: true, maxDependencyNodes: 250, pinnedMethods: ['auto', 'base', 'biocManager', 'github'] };
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'load_settings') return { ...publicSettings, resolveDependencies: true, maxDependencyDepth: 2, includeLightDependencies: false, maxDependencyNodes: 100 };
      if (cmd === 'clear_github_token') return publicSettings;
      return null;
    });

    const { result } = renderHook(() => useSettings(vi.fn()));

    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 10));
    });
    await act(async () => {
      await result.current.clearSavedToken();
    });

    expect(result.current.settings.resolveDependencies).toBe(false);
    expect(result.current.settings.searchConcurrency).toBe(8);
    expect(result.current.settings.archiveGithubMajorGap).toBe(2);
    expect(result.current.settings.maxDependencyDepth).toBe(4);
    expect(result.current.settings.includeLightDependencies).toBe(true);
    expect(result.current.settings.maxDependencyNodes).toBe(250);
  });

  it('磁盘配置迟到返回时，只让用户改动过的字段覆盖磁盘值', async () => {
    const saved = {
      proxy: 'http://127.0.0.1:7890', githubTokenConfigured: false,
      cranMirror: 'https://cloud.r-project.org/', rLibPath: 'D:/R/lib',
      fullSearch: false, searchConcurrency: 8, archiveGithubMajorGap: 2,
      conditional: true, installDependencies: true, showRemoteVersion: true,
      useCache: true, maxCacheEntries: 1000, useFilter: true,
      resolveDependencies: false, maxDependencyDepth: 4,
      includeLightDependencies: true, maxDependencyNodes: 250,
      pinnedMethods: ['auto'], pipIndex: 'https://pypi.org', condaChannels: ['conda-forge'],
    };
    let resolveLoad: ((value: unknown) => void) | undefined;
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'load_settings') {
        return new Promise((resolve) => { resolveLoad = resolve; });
      }
      return saved;
    });

    const { result } = renderHook(() => useSettings(vi.fn()));
    // 模拟 load_settings 尚未返回时用户就改了一个字段。
    act(() => {
      result.current.updateSettingsFromUser((current) => ({ ...current, fullSearch: true }));
    });
    await act(async () => {
      resolveLoad?.(saved);
      await new Promise((resolve) => setTimeout(resolve, 10));
    });

    // 回归：旧实现在此分支整份丢弃磁盘配置，把用户其余已保存设置静默重置为默认值。
    expect(result.current.settings.fullSearch).toBe(true);
    expect(result.current.settings.searchConcurrency).toBe(8);
    expect(result.current.settings.archiveGithubMajorGap).toBe(2);
    expect(result.current.settings.resolveDependencies).toBe(false);
    expect(result.current.settings.maxDependencyDepth).toBe(4);
    expect(result.current.settings.includeLightDependencies).toBe(true);
    expect(result.current.settings.maxDependencyNodes).toBe(250);
    expect(result.current.settings.rLibPath).toBe('D:/R/lib');
  });

  it('should queue a save requested while another save is pending', async () => {
    let resolveFirst: ((value: any) => void) | undefined;
    const saved = { proxy: '', githubTokenConfigured: false, cranMirror: 'https://cloud.r-project.org/', fullSearch: false, searchConcurrency: 6, archiveGithubMajorGap: 1, conditional: true, installDependencies: true, showRemoteVersion: true, useCache: true, maxCacheEntries: 1000, useFilter: true, resolveDependencies: true, maxDependencyDepth: 2, includeLightDependencies: false, maxDependencyNodes: 100, pinnedMethods: ['auto', 'base', 'biocManager', 'github'] };
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'save_settings') {
        if (!resolveFirst) return new Promise((resolve) => { resolveFirst = resolve; });
        return saved;
      }
      return saved;
    });
    const { result } = renderHook(() => useSettings(vi.fn()));
    let first: Promise<void> | undefined;
    act(() => { first = result.current.persistSettings({ fullSearch: true }); });
    act(() => {
      result.current.updateSettingsFromUser((current) => ({ ...current, fullSearch: false }));
      void result.current.persistSettings();
    });
    resolveFirst?.(saved);
    await act(async () => { await first; await new Promise((resolve) => setTimeout(resolve, 0)); });
    expect(vi.mocked(tauriCore.invoke).mock.calls.filter(([cmd]) => cmd === 'save_settings')).toHaveLength(2);
  });
});
