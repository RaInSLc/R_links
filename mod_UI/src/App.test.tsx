import { render, screen, fireEvent, act, waitFor } from '@testing-library/react';
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
    vi.mocked(tauriCore.invoke).mockImplementation(async (cmd) => {
      if (cmd === 'load_history') return [];
      if (cmd === 'load_input_rules') return { separators: [','], commentChars: ['#'], stripQuotes: true, stripCParens: true, splitSpaces: false };
      if (cmd === 'load_settings') return { proxy: '', githubToken: '', cranMirror: '', fullSearch: false, conditional: true, installDependencies: true, showRemoteVersion: true, useCache: true, maxCacheEntries: 1000, useFilter: true };
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
});
