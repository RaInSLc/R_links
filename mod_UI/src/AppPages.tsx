import { NavButton } from "./components";
import { SidebarVersion } from "./SidebarVersion";
import { WorkspaceView } from "./WorkspaceView";
import { ReportView } from "./ReportView";
import { HistoryView } from "./HistoryView";
import { SettingsView } from "./SettingsView";
import type { ComponentProps } from "react";
import type { View, Settings, InputRules } from "./types";
import type { useAppActions } from "./useAppActions";
import type { useSettings } from "./useSettings";

type Actions = ReturnType<typeof useAppActions>;
type SettingsHook = ReturnType<typeof useSettings>;
export type AppPagesProps = Omit<ComponentProps<typeof WorkspaceView>, "onStartSearch" | "onCopyScript" | "onCleanComments" | "onCleanInput" | "onDownloadScript" | "onDownloadPowerShellScript" | "onDownloadBashScript" | "onDownloadSystemRequirements" | "onTempFilter" | "isMethodDisabled">
  & Omit<ComponentProps<typeof ReportView>, "onRetryMissing">
  & Omit<ComponentProps<typeof HistoryView>, "onApplyRecord">
  & Omit<ComponentProps<typeof SettingsView>, "onReplaceSettings" | "onSaveInputRules">
  & Actions & {
    view: View; setView: (view: View) => void; status: string; foundCount: number; summaryProgress: number;
    resultSuggestions: ComponentProps<typeof ReportView>["smartSuggestions"];
    cancelSearchPackage: (name: string) => Promise<boolean>;
    updateAndPersistSettings: (update: (settings: Settings) => Settings) => void;
    replaceSettingsFromUser: SettingsHook["replaceSettingsFromUser"];
    persistSettings: SettingsHook["persistSettings"];
    setStatus: (status: string) => void;
    onInputRulesChange: (rules: InputRules) => void;
  };

export function AppPages(props: AppPagesProps) {
  const { view, setView, status, searching, packageCount, foundCount, results, history, summaryProgress } = props;
  return <div className="app-shell">
    <aside className="sidebar"><div className="brand"><div className="brand-mark">R</div><div><strong>Package Center</strong><span>R 包命令工作台</span></div></div>
      <nav className="nav-list" aria-label="主导航">
        <NavButton active={view === "workspace"} label="工作台" code="01" onClick={() => setView("workspace")} title="Ctrl+1" />
        <NavButton active={view === "report"} label="检索报告" code="02" badge={results.length} onClick={() => setView("report")} title="Ctrl+2" />
        <NavButton active={view === "history"} label="命令历史" code="03" badge={history.length} onClick={() => setView("history")} title="Ctrl+3" />
        <NavButton active={view === "settings"} label="网络设置" code="04" onClick={() => setView("settings")} title="Ctrl+4" />
      </nav>
      <div className="sidebar-summary"><span>当前任务</span><strong>{searching ? `检索中 ${foundCount}/${packageCount}（${packageCount > 0 ? Math.round((foundCount / packageCount) * 100) : 0}%）` : `${packageCount} 个输入`}</strong><progress className="summary-track" value={summaryProgress} max={100} aria-label="已验证包比例" /><small>{results.length ? `${foundCount} 条来源记录` : "等待开始"}</small></div>
      <details className="sidebar-shortcuts"><summary>快捷键</summary><div className="shortcut-list"><kbd>Ctrl</kbd>+<kbd>1</kbd>~<kbd>4</kbd> <span>切换视图</span><kbd>Ctrl</kbd>+<kbd>↵</kbd> <span>开始检索</span><kbd>Ctrl</kbd>+<kbd>⇧</kbd>+<kbd>C</kbd> <span>复制脚本</span><kbd>Ctrl</kbd>+<kbd>S</kbd> <span>下载脚本</span><kbd>Ctrl</kbd>+<kbd>⇧</kbd>+<kbd>K</kbd> <span>清空输入</span><kbd>Ctrl</kbd>+<kbd>D</kbd> <span>去重</span><kbd>Ctrl</kbd>+<kbd>F</kbd> <span>搜索</span><kbd>Alt</kbd>+<kbd>1</kbd>/<kbd>2</kbd> <span>切换图/列表</span></div></details>
      <SidebarVersion
        appVersion={props.appVersion}
        updateState={props.updateState}
        updateStage={props.updateStage}
        updaterConfig={props.updaterConfig}
        onOpenUpdateSettings={() => setView("settings")}
      />
    </aside>
    <main className="main-area"><header className="topbar"><div><span className="eyebrow">R PACKAGE INSTALLATION</span><h1>{view === "workspace" ? "安装命令工作台" : view === "report" ? "多源检索报告" : view === "history" ? "命令历史" : "网络与镜像设置"}</h1></div><div key={status} className={`status-chip status-pulse ${searching ? "active" : ""}`} role="status" aria-live="polite" aria-atomic="true"><i aria-hidden="true" />{status}</div></header>
      <section className="content">{view === "workspace" && <WorkspaceView {...props} onInputChange={props.acceptInputValue} onPaste={props.pasteInput} onClear={() => props.acceptInputValue("", "manual")} onStartSearch={props.handleStartSearch} onCopyScript={props.copyScript} onCleanComments={props.cleanComments} onCleanInput={props.cleanInput} onDownloadScript={props.downloadScript} onDownloadPowerShellScript={() => props.downloadWrapperScript("powershell")} onDownloadBashScript={() => props.downloadWrapperScript("bash")} onDownloadSystemRequirements={props.downloadSystemRequirements} onTempFilter={props.handleTempFilter} isMethodDisabled={props.isMethodDisabled} />}
        {view === "report" && <ReportView {...props} smartSuggestions={props.resultSuggestions} onApplySmartSuggestion={(s) => { if (s.action === "openSettings") setView("settings"); else if (s.action === "enableFullSearch") props.updateAndPersistSettings((c) => ({ ...c, fullSearch: true })); else if (s.action === "retrySearch") props.handleStartSearch(); props.setStatus(`已应用智能建议：${s.title}`); }} onRetryMissing={(packages: string[]) => { props.acceptInputValue(packages.join("\n"), "manual"); setView("workspace"); props.setStatus(`已回填 ${packages.length} 个未找到的包名，可重新检索`); }} onCancelPackage={props.cancelSearchPackage} />}
        {view === "history" && <HistoryView {...props} onApplyRecord={props.applyHistoryRecord} />}
        {view === "settings" && <SettingsView {...props} onSaveSettings={props.persistSettings} onReplaceSettings={(next) => { props.replaceSettingsFromUser(next); void props.persistSettings(next); }} onCheckUpdates={props.onCheckUpdates} onClearCache={props.onClearCache} onExportDiagnostics={props.onExportDiagnostics} onSaveInputRules={props.saveInputRules} onReplaceInputRules={(next) => { props.onInputRulesChange(next); void props.saveInputRules(next); }} />}
      </section>
    </main>
  </div>;
}
