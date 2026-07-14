import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { SettingsView } from './SettingsView';
import { vi, describe, it, expect } from 'vitest';
import '@testing-library/jest-dom';
import { defaultSettings } from './types';

describe('SettingsView Component', () => {
  const createProps = () => ({
    settings: {
      proxy: '',
      githubToken: '',
      cranMirror: 'https://cloud.r-project.org',
      fullSearch: false,
      conditional: true,
      installDependencies: true,
      showRemoteVersion: true,
      useCache: true,
      maxCacheEntries: 1000,
      useFilter: true,
      resolveDependencies: true,
      maxDependencyDepth: 2,
      includeLightDependencies: false,
      maxDependencyNodes: 100,
      pinnedMethods: [...defaultSettings.pinnedMethods],
    },
    tokenConfigured: false,
    showToken: false,
    settingsBusy: false,
    currentTheme: 'office',
    currentFont: 'modern',
    checkingUpdate: false,
    updateState: 'idle' as const,
    updateMessage: '',
    appVersion: '0.1.9',
    updateVersion: '',
    onProxyChange: vi.fn(),
    onTokenChange: vi.fn(),
    onTokenToggle: vi.fn(),
    onClearToken: vi.fn(),
    onFullSearchChange: vi.fn(),
    onConditionalChange: vi.fn(),
    onInstallDependenciesChange: vi.fn(),
    onShowRemoteVersionChange: vi.fn(),
    onUseCacheChange: vi.fn(),
    onUseFilterChange: vi.fn(),
    onMaxCacheEntriesChange: vi.fn(),
    onCranMirrorChange: vi.fn(),
    onMirrorSelect: vi.fn(),
    onResolveDependenciesChange: vi.fn(),
    onMaxDependencyDepthChange: vi.fn(),
    onIncludeLightDependenciesChange: vi.fn(),
    onMaxDependencyNodesChange: vi.fn(),
    onSaveSettings: vi.fn(),
    onReplaceSettings: vi.fn(),
    onThemeChange: vi.fn(),
    onFontChange: vi.fn(),
    currentFontSize: 14,
    onFontSizeChange: vi.fn(),
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
    onReplaceInputRules: vi.fn(),
    onSaveInputRules: vi.fn(),
    inputRulesBusy: false,
  });

  it('点击“保存过滤规则”按钮时，应触发 onSaveInputRules 回调', () => {
    const props = createProps();
    render(<SettingsView {...props} />);
    fireEvent.click(screen.getByText('输入过滤'));
    const saveRulesBtn = screen.getByText('保存过滤规则');
    expect(saveRulesBtn).toBeInTheDocument();
    fireEvent.click(saveRulesBtn);
    expect(props.onSaveInputRules).toHaveBeenCalledTimes(1);
  });

  it('点击“保存设置”按钮时，应触发 onSaveSettings 回调', () => {
    const props = createProps();
    render(<SettingsView {...props} />);
    fireEvent.click(screen.getByText('网络连接'));
    const saveSettingsBtn = screen.getByText('保存设置');
    expect(saveSettingsBtn).toBeInTheDocument();
    fireEvent.click(saveSettingsBtn);
    expect(props.onSaveSettings).toHaveBeenCalledTimes(1);
  });

  it('应展示当前版本和更新状态', () => {
    const props = createProps();
    render(<SettingsView {...props} updateState="readyToRestart" updateMessage="更新安装成功" updateVersion="0.2.0" />);
    fireEvent.click(screen.getByText('界面与系统'));

    expect(screen.getByText(/当前版本 0.1.9/)).toBeInTheDocument();
    expect(screen.getByText(/状态：待重启/)).toBeInTheDocument();
    expect(screen.getByText(/目标版本：0.2.0/)).toBeInTheDocument();
  });

  it('恢复默认时应使用可选字体值并一次性替换设置', () => {
    const props = createProps();
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    render(<SettingsView {...props} />);
    fireEvent.click(screen.getByText('配置备份'));

    fireEvent.click(screen.getByText('恢复默认'));

    expect(props.onReplaceSettings).toHaveBeenCalledWith(expect.objectContaining({
      resolveDependencies: true,
      maxDependencyDepth: 2,
      includeLightDependencies: false,
      maxDependencyNodes: 100,
    }));
    expect(props.onFontChange).toHaveBeenCalledWith('system');
    expect(props.onSaveSettings).not.toHaveBeenCalled();
  });

  it('导入配置时应裁剪设置范围并过滤非法字段', async () => {
    const props = createProps();
    const { container } = render(<SettingsView {...props} />);
    const file = {
      text: async () => JSON.stringify({
        settings: {
          fullSearch: 'yes',
          maxCacheEntries: 20000,
          maxDependencyDepth: 9,
          maxDependencyNodes: 999,
          pinnedMethods: ['github', 'invalid', 'github', 'base'],
        },
        theme: 'bad-theme',
        fontFamily: 'classic',
        fontSize: 30,
        inputRules: {
          separators: [',', '', '::'],
          stripQuotes: 'bad',
          excludeRegex: ['(', '^library\\('],
          excludeKeywords: ['library', 'require'],
        },
      }),
    };

    fireEvent.click(screen.getByText('配置备份'));
    const fileInput = container.querySelector('input[type="file"][accept=".json"]') as HTMLInputElement;
    fireEvent.change(fileInput, { target: { files: [file] } });

    await waitFor(() => expect(props.onReplaceSettings).toHaveBeenCalled());
    expect(props.onReplaceSettings).toHaveBeenCalledWith(expect.objectContaining({
      fullSearch: false,
      maxCacheEntries: 10000,
      maxDependencyDepth: 5,
      maxDependencyNodes: 500,
      pinnedMethods: ['github', 'base'],
    }));
    expect(props.onThemeChange).not.toHaveBeenCalled();
    expect(props.onFontChange).toHaveBeenCalledWith('classic');
    expect(props.onFontSizeChange).not.toHaveBeenCalled();
    expect(props.onReplaceInputRules).toHaveBeenCalledWith(expect.objectContaining({
      separators: [',', '::'],
      stripQuotes: true,
      excludeRegex: ['^library\\('],
      excludeKeywords: ['library', 'require'],
    }));
  });

  it('缓存菜单中切换包结果缓存时应触发回调且不白屏', () => {
    const props = createProps();
    render(<SettingsView {...props} />);

    fireEvent.click(screen.getByText('缓存'));
    fireEvent.click(screen.getByText('使用包结果缓存'));

    expect(props.onUseCacheChange).toHaveBeenCalledWith(false);
    expect(screen.getByText('包结果缓存')).toBeInTheDocument();
  });
});
