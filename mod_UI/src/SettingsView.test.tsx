import { render, screen, fireEvent } from '@testing-library/react';
import { SettingsView } from './SettingsView';
import { vi, describe, it, expect } from 'vitest';
import '@testing-library/jest-dom';

describe('SettingsView Component', () => {
  const defaultProps = {
    settings: {
      proxy: '',
      githubToken: '',
      cranMirror: 'https://cloud.r-project.org',
      fullSearch: false,
      conditional: true,
      installDependencies: true,
      showRemoteVersion: true,
    },
    tokenConfigured: false,
    showToken: false,
    settingsBusy: false,
    currentTheme: 'office',
    currentFont: 'modern',
    checkingUpdate: false,
    updateMessage: '',
    onProxyChange: vi.fn(),
    onTokenChange: vi.fn(),
    onTokenToggle: vi.fn(),
    onClearToken: vi.fn(),
    onFullSearchChange: vi.fn(),
    onConditionalChange: vi.fn(),
    onInstallDependenciesChange: vi.fn(),
    onShowRemoteVersionChange: vi.fn(),
    onCranMirrorChange: vi.fn(),
    onMirrorSelect: vi.fn(),
    onSaveSettings: vi.fn(),
    onThemeChange: vi.fn(),
    onFontChange: vi.fn(),
    onCheckUpdates: vi.fn(),
    onClearCache: vi.fn(),
    onExportDiagnostics: vi.fn(),
    inputRules: {
      separators: [',', ';'],
      commentChars: ['#'],
      stripQuotes: true,
      stripCParens: true,
      splitSpaces: false,
      excludeRegex: [],
      excludeKeywords: [],
    },
    onInputRulesChange: vi.fn(),
    onSaveInputRules: vi.fn(),
    inputRulesBusy: false,
  };

  it('点击“保存过滤规则”按钮时，应触发 onSaveInputRules 回调', () => {
    render(<SettingsView {...defaultProps} />);
    const saveRulesBtn = screen.getByText('保存过滤规则');
    expect(saveRulesBtn).toBeInTheDocument();
    fireEvent.click(saveRulesBtn);
    expect(defaultProps.onSaveInputRules).toHaveBeenCalledTimes(1);
  });

  it('点击“保存设置”按钮时，应触发 onSaveSettings 回调', () => {
    render(<SettingsView {...defaultProps} />);
    const saveSettingsBtn = screen.getByText('保存设置');
    expect(saveSettingsBtn).toBeInTheDocument();
    fireEvent.click(saveSettingsBtn);
    expect(defaultProps.onSaveSettings).toHaveBeenCalledTimes(1);
  });
});
