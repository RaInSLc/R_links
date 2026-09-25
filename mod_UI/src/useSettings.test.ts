import { renderHook, act } from '@testing-library/react';
import { useSettings, settingsFromPublicSettings } from './useSettings';
import type { Settings } from './types';
import { defaultSettings } from './types';
import type { PublicSettings } from './utils';
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

  it('清除 Token 不覆盖其他界面设置', async () => {
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

    expect(result.current.settings.resolveDependencies).toBe(true);
    expect(result.current.settings.searchConcurrency).toBe(8);
    expect(result.current.settings.archiveGithubMajorGap).toBe(2);
    expect(result.current.settings.maxDependencyDepth).toBe(2);
    expect(result.current.settings.includeLightDependencies).toBe(false);
    expect(result.current.settings.maxDependencyNodes).toBe(100);
  });

  it('清除 Token 期间排队的保存会使用最新设置执行', async () => {
    let finishClear!: (value: unknown) => void;
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'clear_github_token') return new Promise(resolve => { finishClear = resolve; });
      if (cmd === 'save_settings') return { ...(args as any).settings, githubTokenConfigured: false };
      return { ...defaultSettings, githubTokenConfigured: true };
    });
    const { result } = renderHook(() => useSettings(vi.fn()));
    await act(async () => {});
    let clearing!: Promise<void>;
    act(() => { clearing = result.current.clearSavedToken(); });
    act(() => result.current.updateSettingsFromUser(current => ({ ...current, searchConcurrency: 7 })));
    await act(async () => { await result.current.persistSettings(); });
    await act(async () => { finishClear({}); await clearing; });
    expect(tauriCore.invoke).toHaveBeenCalledWith('save_settings', {
      settings: expect.objectContaining({ searchConcurrency: 7 }),
    });
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

describe('settingsFromPublicSettings 字段映射完备性', () => {
  // 字段齐全、取值互不相同的 PublicSettings，标注为 PublicSettings 类型：
  // 一旦该接口新增字段而此处未同步补齐，TypeScript 编译会直接失败；
  // 下面基于 Object.keys 的逐字段断言则保证“新增字段但漏加映射”被运行时捕获。
  const publicSettings: PublicSettings = {
    proxy: 'http://127.0.0.1:7890',
    githubTokenConfigured: true,
    cranMirror: 'https://cloud.r-project.org/',
    rLibPath: 'D:/R/library-42',
    fullSearch: true,
    searchConcurrency: 7,
    archiveGithubMajorGap: 3,
    conditional: false,
    installDependencies: false,
    showRemoteVersion: false,
    useCache: false,
    maxCacheEntries: 4321,
    useFilter: false,
    resolveDependencies: false,
    maxDependencyDepth: 5,
    includeLightDependencies: true,
    maxDependencyNodes: 321,
    pinnedMethods: ['biocManager', 'github'],
    pipIndex: 'https://pypi.org/simple',
    condaChannels: ['conda-forge', 'bioconda'],
  };

  it('把 PublicSettings 的每个字段都映射进 Settings，且取值一一对应', () => {
    const mapped = settingsFromPublicSettings(publicSettings);
    type StringKeyed = Record<string, unknown>;

    // PublicSettings 除 githubTokenConfigured 外的每个字段都应原样进入 Settings。
    (Object.keys(publicSettings) as Array<keyof PublicSettings>).forEach((key) => {
      if (key === 'githubTokenConfigured') return;
      expect((mapped as unknown as StringKeyed)[key]).toStrictEqual(
        (publicSettings as unknown as StringKeyed)[key],
      );
    });

    // githubToken 是只写字段：后端不下发明文 token，映射后恒为空字符串。
    expect(mapped.githubToken).toBe('');
    expect(publicSettings.githubTokenConfigured).toBe(true);
  });

  it('映射结果恰好覆盖 Settings 的全部字段，不遗漏也不多余', () => {
    const mapped = settingsFromPublicSettings(publicSettings);

    expect(Object.keys(mapped).sort()).toEqual(Object.keys(defaultSettings).sort());
    Object.keys(defaultSettings).forEach((key) => {
      // 任一字段若因漏加映射而缺失或为 undefined，这里立刻失败。
      expect(mapped[key as keyof Settings]).not.toBeUndefined();
    });
  });

  it('加载路径会用磁盘值填充所有被映射的字段', async () => {
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'load_settings') return publicSettings;
      return null;
    });

    const { result } = renderHook(() => useSettings(vi.fn()));
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 10));
    });

    expect(result.current.settings.proxy).toBe(publicSettings.proxy);
    expect(result.current.settings.cranMirror).toBe(publicSettings.cranMirror);
    expect(result.current.settings.rLibPath).toBe(publicSettings.rLibPath);
    expect(result.current.settings.fullSearch).toBe(publicSettings.fullSearch);
    expect(result.current.settings.searchConcurrency).toBe(publicSettings.searchConcurrency);
    expect(result.current.settings.archiveGithubMajorGap).toBe(publicSettings.archiveGithubMajorGap);
    expect(result.current.settings.maxCacheEntries).toBe(publicSettings.maxCacheEntries);
    expect(result.current.settings.resolveDependencies).toBe(publicSettings.resolveDependencies);
    expect(result.current.settings.maxDependencyDepth).toBe(publicSettings.maxDependencyDepth);
    expect(result.current.settings.includeLightDependencies).toBe(publicSettings.includeLightDependencies);
    expect(result.current.settings.maxDependencyNodes).toBe(publicSettings.maxDependencyNodes);
    expect(result.current.settings.pinnedMethods).toStrictEqual(publicSettings.pinnedMethods);
    expect(result.current.settings.pipIndex).toBe(publicSettings.pipIndex);
    expect(result.current.settings.condaChannels).toStrictEqual(publicSettings.condaChannels);
  });
});
