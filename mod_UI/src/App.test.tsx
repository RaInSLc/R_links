import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import App from './App';
import { vi, describe, it, expect, beforeEach } from 'vitest';
import * as tauriCore from '@tauri-apps/api/core';
import '@testing-library/jest-dom';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

describe('App Component Input Validation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.stubGlobal('navigator', {
      ...navigator,
      clipboard: {
        writeText: vi.fn().mockResolvedValue(undefined),
      },
    });
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'load_history') return [];
      if (cmd === 'load_input_rules') return { separators: [','], commentChars: ['#'], stripQuotes: true, stripCParens: true, splitSpaces: false };
      if (cmd === 'load_settings') return { proxy: '', githubToken: '', cranMirror: '', fullSearch: false, conditional: true, installDependencies: true, showRemoteVersion: true, useCache: true, maxCacheEntries: 1000, useFilter: true, resolveDependencies: true, maxDependencyDepth: 2, includeLightDependencies: false, maxDependencyNodes: 100 };
      if (cmd === 'generate_script') return 'install.packages("ggplot2")';
      return null;
    });
  });

  it('如果输入包含非法控制字符，应当拒绝并显示状态提示', async () => {
    render(<App />);

    const textarea = screen.getByLabelText('R 包输入列表');
    expect(textarea).toBeInTheDocument();

    fireEvent.change(textarea, { target: { value: 'ggplot2\u0000' } });

    await waitFor(() => {
      const statusChip = screen.getByRole('status');
      expect(statusChip).toHaveTextContent(/超出限制或包含非法字符/);
    });
    expect(textarea).toHaveValue('');
  });

  it('如果单行字节超出限制，应当拒绝输入', async () => {
    render(<App />);

    const textarea = screen.getByLabelText('R 包输入列表');
    
    const longLine = 'a'.repeat(2049);
    fireEvent.change(textarea, { target: { value: longLine } });

    await waitFor(() => {
      const statusChip = screen.getByRole('status');
      expect(statusChip).toHaveTextContent(/超出限制或包含非法字符/);
    });
    expect(textarea).toHaveValue('');
  });

  it('报告页复制安装指令失败时，应当显示状态提示', async () => {
    localStorage.setItem('rlinks_input', 'ggplot2');
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'load_history') return [];
      if (cmd === 'load_input_rules') return { separators: [','], commentChars: ['#'], stripQuotes: true, stripCParens: true, splitSpaces: false };
      if (cmd === 'load_settings') return { proxy: '', githubToken: '', cranMirror: '', fullSearch: false, conditional: true, installDependencies: true, showRemoteVersion: true, useCache: true, maxCacheEntries: 1000, useFilter: true, resolveDependencies: true, maxDependencyDepth: 2, includeLightDependencies: false, maxDependencyNodes: 100 };
      if (cmd === 'load_cached_results') return [{ package: 'ggplot2', requestedVersion: '', latestVersion: '3.5.0', repository: '', realName: 'ggplot2', source: 'cran', found: true, message: '缓存命中', status: 'found' }];
      if (cmd === 'generate_script') return 'install.packages("ggplot2")';
      return null;
    });
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(new Error('clipboard denied'));

    render(<App />);
    fireEvent.click(screen.getByText('检索报告'));

    const copyButton = await screen.findByTitle(/复制安装指令/);
    fireEvent.click(copyButton);

    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent('复制安装指令失败: clipboard denied');
    });
  });

  it('切换安装后验证时，应当重新生成带验证选项的脚本', async () => {
    render(<App />);

    fireEvent.change(screen.getByLabelText('R 包输入列表'), { target: { value: 'ggplot2' } });

    await waitFor(() => {
      expect(vi.mocked(tauriCore.invoke)).toHaveBeenCalledWith(
        'generate_script',
        expect.objectContaining({
          options: expect.objectContaining({ appendVerify: false }),
        }),
      );
    });

    fireEvent.click(screen.getByText('安装后验证'));

    await waitFor(() => {
      expect(vi.mocked(tauriCore.invoke)).toHaveBeenCalledWith(
        'generate_script',
        expect.objectContaining({
          options: expect.objectContaining({ appendVerify: true }),
        }),
      );
    });
  });
});
