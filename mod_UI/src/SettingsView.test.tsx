import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { SettingsView } from './SettingsView';
import { vi, describe, it, expect } from 'vitest';
import '@testing-library/jest-dom';
import { defaultSettings } from './types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (command: string) => command === 'load_package_cache' ? [] : undefined),
}));

describe('SettingsView Component', () => {
  const createProps = () => ({
    settings: {
      proxy: '',
      githubToken: '',
      cranMirror: 'https://cloud.r-project.org',
      rLibPath: '',
      fullSearch: false,
      searchConcurrency: 6,
      archiveGithubMajorGap: 1,
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
      pipIndex: defaultSettings.pipIndex,
      condaChannels: [...defaultSettings.condaChannels],
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
    onSearchConcurrencyChange: vi.fn(),
    onArchiveGithubMajorGapChange: vi.fn(),
    onConditionalChange: vi.fn(),
    onInstallDependenciesChange: vi.fn(),
    onShowRemoteVersionChange: vi.fn(),
    onUseCacheChange: vi.fn(),
    onUseFilterChange: vi.fn(),
    onMaxCacheEntriesChange: vi.fn(),
     onCranMirrorChange: vi.fn(),
     onRLibPathChange: vi.fn(),
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

  it('编辑排除正则时应保留换行位置', () => {
    const props = createProps();
    render(<SettingsView {...props} />);
    fireEvent.click(screen.getByText('输入过滤'));

    const textarea = screen.getByPlaceholderText('例如: ^library\\( 或 ^install\\.packages\\(');
    fireEvent.change(textarea, { target: { value: '^library\\(\n' } });

    expect(props.onInputRulesChange).toHaveBeenCalledWith(expect.objectContaining({
      excludeRegex: ['^library\\(', ''],
    }));
  });

  it('编辑空格分隔规则时应保留当前尾随空格', () => {
    const props = createProps();
    render(<SettingsView {...props} />);
    fireEvent.click(screen.getByText('输入过滤'));

    fireEvent.change(screen.getByPlaceholderText(', ;'), { target: { value: ', ' } });
    fireEvent.change(screen.getByPlaceholderText('#'), { target: { value: '# ' } });
    fireEvent.change(screen.getByPlaceholderText('例如: library require if else'), { target: { value: 'library ' } });

    expect(props.onInputRulesChange).toHaveBeenNthCalledWith(1, expect.objectContaining({ separators: [',', ''] }));
    expect(props.onInputRulesChange).toHaveBeenNthCalledWith(2, expect.objectContaining({ commentChars: ['#', ''] }));
    expect(props.onInputRulesChange).toHaveBeenNthCalledWith(3, expect.objectContaining({ excludeKeywords: ['library', ''] }));
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
      searchConcurrency: 6,
      archiveGithubMajorGap: 1,
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
          searchConcurrency: 99,
          archiveGithubMajorGap: 99,
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
      searchConcurrency: 12,
      archiveGithubMajorGap: 10,
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

  it('缓存菜单可以刷新条目列表', async () => {
    const props = createProps();
    render(<SettingsView {...props} />);

    fireEvent.click(screen.getByText('缓存'));
    fireEvent.click(screen.getByText('刷新列表'));

    await waitFor(() => expect(screen.getByText('0 条')).toBeInTheDocument());
  });

  it('网络设置中调整搜索并发上限时应触发回调', () => {
    const props = createProps();
    render(<SettingsView {...props} />);
    fireEvent.click(screen.getByText('网络连接'));

    const concurrencyInput = screen.getByLabelText('搜索并发上限');
    fireEvent.change(concurrencyInput, { target: { value: '8' } });

    expect(props.onSearchConcurrencyChange).toHaveBeenCalledWith(8);
  });

  it('策略设置中调整 Archive 转 GitHub 阈值时应触发回调', () => {
    const props = createProps();
    render(<SettingsView {...props} />);
    fireEvent.click(screen.getByText('检索策略'));

    const gapInput = screen.getByLabelText('Archive 转 GitHub 主版本差阈值');
    fireEvent.change(gapInput, { target: { value: '2' } });

    expect(props.onArchiveGithubMajorGapChange).toHaveBeenCalledWith(2);
  });

  it('网络设置中编辑 R 库路径时应触发回调', () => {
    const props = createProps();
    render(<SettingsView {...props} />);
    fireEvent.change(screen.getByPlaceholderText('留空使用当前 R 默认库路径'), { target: { value: 'D:/R/project-library' } });

    expect(props.onRLibPathChange).toHaveBeenCalledWith('D:/R/project-library');
  });

  it('网络设置中可以运行 R 编译环境 Doctor', async () => {
    const props = createProps();
    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'check_system_toolchain') return [{ tool: 'Rscript', available: true, version: 'Rscript version 4.4.0', advice: '' }];
      return command === 'load_package_cache' ? [] : undefined;
    });
    render(<SettingsView {...props} />);
    fireEvent.click(screen.getByText('缓存'));
    fireEvent.click(screen.getByText('检查编译环境'));

    await waitFor(() => expect(screen.getByText(/Rscript version 4.4.0/)).toBeInTheDocument());
  });
});
