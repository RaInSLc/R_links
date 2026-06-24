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
    const mockSettings = { fullSearch: true, proxy: '127.0.0.1:8080', cranMirror: '', githubTokenConfigured: false, githubToken: '' };
    
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
});
