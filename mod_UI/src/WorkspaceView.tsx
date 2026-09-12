import { useRef, useState } from "react";
import { EcosystemSourceConfig } from "./EcosystemSourceConfig";
import { PackageInputEditor, type PackageInputEditorHandle } from "./PackageInputEditor";
import { ScriptPreview } from "./ScriptPreview";
import { WorkspaceInputActions } from "./WorkspaceInputActions";
import { WorkspaceInputSummary } from "./WorkspaceInputSummary";
import { WorkspaceStrategyPanel } from "./WorkspaceStrategyPanel";
import { PanelHeader } from "./components";
import type { Ecosystem, Method, Settings } from "./types";
import { MAX_INPUT_CHARS, MAX_PACKAGE_LINES, type SmartSuggestion } from "./utils";

interface WorkspaceViewProps {
  input: string;
  inputTooLarge: boolean;
  inputProfile: { total: number; archiveUrls: number; repositories: number };
  method: Method;
  ecosystem?: Ecosystem;
  pipIndex?: string;
  condaChannels?: string[];
  rBinaryMirror?: string;
  onEcosystemChange?: (value: Ecosystem) => void;
  onPipIndexChange?: (value: string) => void;
  onCondaChannelsChange?: (value: string[]) => void;
  onRBinaryMirrorChange?: (value: string) => void;
  conditional: boolean;
  installDependencies: boolean;
  showRemoteVersion: boolean;
  verifyInstall: boolean;
  parallelInstall: boolean;
  settings: Settings;
  smartSuggestions: SmartSuggestion[];
  script: string;
  scriptTooLarge: boolean;
  scriptCommandCount: number;
  duplicateCount: number;
  searching: boolean;
  paused?: boolean;
  openingSearchTabs: boolean;
  onInputChange: (value: string, source: "manual" | "clipboard") => string;
  onPaste: () => void;
  onClear: () => void;
  onOpenSearchTabs: () => void;
  onStartSearch: () => void;
  onStopSearch: () => void;
  onTogglePause?: () => void;
  onMethodChange: (method: Method) => void;
  pinnedMethods: Method[];
  onPinnedMethodsChange: (methods: Method[]) => void;
  onApplySmartSuggestion: (suggestion: SmartSuggestion) => void;
  onConditionalChange: (value: boolean) => void;
  onInstallDependenciesChange: (value: boolean) => void;
  onShowRemoteVersionChange: (value: boolean) => void;
  onVerifyInstallChange: (value: boolean) => void;
  onParallelInstallChange: (value: boolean) => void;
  onFullSearchChange: (value: boolean) => void;
  onUseCacheChange: (value: boolean) => void;
  onTempFilter: (text: string, mode: "chars" | "lines") => void;
  onCopyScript: () => void;
  onCleanComments: () => void;
  onDownloadScript: () => void;
  onDownloadPowerShellScript: () => void;
  onDownloadBashScript: () => void;
  onDownloadSystemRequirements: (kind: "bash" | "powershell") => void;
  copyWithLineNumbers: boolean;
  onCopyWithLineNumbersChange: (value: boolean) => void;
  isMethodDisabled: (candidate: Method) => boolean;
}

export function WorkspaceView({
  input, inputTooLarge, inputProfile, method,
  conditional, installDependencies, showRemoteVersion, verifyInstall, parallelInstall, settings,
  ecosystem = "r", pipIndex = "", condaChannels = [], rBinaryMirror = "",
  onEcosystemChange = () => {}, onPipIndexChange = () => {}, onCondaChannelsChange = () => {}, onRBinaryMirrorChange = () => {},
  smartSuggestions, script, scriptTooLarge, scriptCommandCount, duplicateCount,
  searching, paused, openingSearchTabs, onInputChange, onPaste, onClear, onOpenSearchTabs, onStartSearch, onStopSearch,
  onMethodChange, pinnedMethods, onPinnedMethodsChange, onApplySmartSuggestion, onConditionalChange, onInstallDependenciesChange,
  onShowRemoteVersionChange, onVerifyInstallChange, onParallelInstallChange, onFullSearchChange, onUseCacheChange, onTempFilter,
  onCopyScript, onCleanComments, onDownloadScript, onDownloadPowerShellScript, onDownloadBashScript, onDownloadSystemRequirements,
  onTogglePause = () => {}, copyWithLineNumbers, onCopyWithLineNumbersChange, isMethodDisabled,
}: WorkspaceViewProps) {
  const [pasteHint, setPasteHint] = useState(false);
  const inputEditorRef = useRef<PackageInputEditorHandle>(null);

  return (
    <div className="workspace-grid">
      <section className="panel input-panel">
        <PanelHeader step="01" title="输入包列表" meta={`${inputProfile.total}/${MAX_PACKAGE_LINES} 项${duplicateCount > 0 ? ` · ${duplicateCount} 重复` : ""} · ${new Blob([input]).size}/${MAX_INPUT_CHARS}B`} />
        <EcosystemSourceConfig
          ecosystem={ecosystem} pipIndex={pipIndex} condaChannels={condaChannels} rBinaryMirror={rBinaryMirror} searching={searching}
          onEcosystemChange={onEcosystemChange} onPipIndexChange={onPipIndexChange}
          onCondaChannelsChange={onCondaChannelsChange} onRBinaryMirrorChange={onRBinaryMirrorChange}
        />
        <PackageInputEditor
          ref={inputEditorRef} input={input} inputTooLarge={inputTooLarge} searching={searching}
          onInputChange={onInputChange} onStartSearch={onStartSearch} onPasteIssues={() => setPasteHint(true)}
        />
        <WorkspaceInputSummary
          input={input} inputTooLarge={inputTooLarge} inputProfile={inputProfile} ecosystem={ecosystem}
          settings={settings} duplicateCount={duplicateCount} smartSuggestions={smartSuggestions} searching={searching}
          pasteHint={pasteHint} onInputChange={onInputChange} onApplySmartSuggestion={onApplySmartSuggestion}
          onCleanComments={onCleanComments} onDismissPasteHint={() => setPasteHint(false)}
        />
        <WorkspaceInputActions
          input={input} inputTooLarge={inputTooLarge} duplicateCount={duplicateCount} searching={searching} paused={paused}
          openingSearchTabs={openingSearchTabs} onInputChange={onInputChange} onPaste={onPaste} onClear={onClear}
          onImportFile={() => inputEditorRef.current?.openFilePicker()} onOpenSearchTabs={onOpenSearchTabs}
          onStartSearch={onStartSearch} onStopSearch={onStopSearch} onTogglePause={onTogglePause} onTempFilter={onTempFilter}
        />
      </section>
      <WorkspaceStrategyPanel
        ecosystem={ecosystem} method={method} settings={settings} conditional={conditional}
        installDependencies={installDependencies} showRemoteVersion={showRemoteVersion} verifyInstall={verifyInstall}
        parallelInstall={parallelInstall} searching={searching} pinnedMethods={pinnedMethods} onMethodChange={onMethodChange}
        onPinnedMethodsChange={onPinnedMethodsChange} onConditionalChange={onConditionalChange}
        onInstallDependenciesChange={onInstallDependenciesChange} onShowRemoteVersionChange={onShowRemoteVersionChange}
        onVerifyInstallChange={onVerifyInstallChange} onParallelInstallChange={onParallelInstallChange}
        onFullSearchChange={onFullSearchChange} onUseCacheChange={onUseCacheChange} isMethodDisabled={isMethodDisabled}
      />
      <ScriptPreview
        ecosystem={ecosystem} script={script} scriptTooLarge={scriptTooLarge} scriptCommandCount={scriptCommandCount}
        copyWithLineNumbers={copyWithLineNumbers} onCopyWithLineNumbersChange={onCopyWithLineNumbersChange}
        onCleanComments={onCleanComments} onDownloadScript={onDownloadScript}
        onDownloadPowerShellScript={onDownloadPowerShellScript} onDownloadBashScript={onDownloadBashScript}
        onDownloadSystemRequirements={onDownloadSystemRequirements} onCopyScript={onCopyScript}
      />
    </div>
  );
}
