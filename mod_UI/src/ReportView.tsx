import { useCallback, useEffect, useMemo, useState } from "react";
import { PanelHeader } from "./components";
import { DependencyGraphView, DependencyListView } from "./DependencyViews";
import type { DependencyGraph, SearchResult, SmartSuggestion } from "./utils";
import { isPlainMissingResult, uniquePackages } from "./reportUtils";
import { ReportActions as ReportActionsBase } from "./ReportActions";
import type { ComponentProps } from "react";
import { ReportLogs } from "./ReportLogs";
import { ReportOverview } from "./ReportOverview";
import { ReportResultsTable } from "./ReportResultsTable";
import { invoke } from "@tauri-apps/api/core";
import { defaultSettings, type Settings } from "./types";

export interface ReportViewProps {
  results: SearchResult[];
  logs: string[];
  dependencyGraph: DependencyGraph | null;
  packageCount: number;
  uniqueFoundCount: number;
  smartSuggestions: SmartSuggestion[];
  searching: boolean;
  searchDuration: number | null;
  stageTimings?: Array<{ stage: string; durationMs: number }>;
  onClearLogs: () => void;
  onStatusChange: (status: string) => void;
  onApplySmartSuggestion: (suggestion: SmartSuggestion) => void;
  onRetryMissing: (packages: string[]) => void;
  onCancelPackage?: (packageName: string) => void;
}

export function ReportView({
  results: rawResults,
  logs,
  dependencyGraph,
  packageCount,
  uniqueFoundCount,
  smartSuggestions,
  searching,
  searchDuration,
  stageTimings = [],
  onClearLogs,
  onStatusChange,
  onApplySmartSuggestion,
  onRetryMissing,
  onCancelPackage,
  settings = defaultSettings,
}: ReportViewProps & { settings?: Settings }) {
  const [generatedCommands, setGeneratedCommands] = useState<{ results: SearchResult[]; settings: Settings; commands: string[] } | null>(null);
  useEffect(() => {
    let active = true;
    if (!rawResults.some((result) => result.found && !["pip", "conda"].includes(result.source))) return;
    const timer = window.setTimeout(() => {
      void invoke<string[]>("generate_result_commands", { options: { method: "auto", conditional: settings.conditional, installDependencies: settings.installDependencies, mirror: settings.cranMirror, rLibPath: settings.rLibPath, archiveGithubMajorGap: settings.archiveGithubMajorGap, appendVerify: false, parallelInstall: false }, results: rawResults, showRemoteVersion: settings.showRemoteVersion }).then((commands) => {
        if (active && Array.isArray(commands) && commands.length === rawResults.length) setGeneratedCommands({ results: rawResults, settings, commands });
      }).catch(() => { if (active) onStatusChange("报告安装命令生成失败，请在工作台检查输入"); });
    }, 150);
    return () => { active = false; window.clearTimeout(timer); };
  }, [rawResults, settings, onStatusChange]);
  const results = useMemo(() => rawResults.map((result, index) => {
    if (!result.found || ["pip", "conda"].includes(result.source)) return result;
    const ready = generatedCommands?.results === rawResults && generatedCommands.settings === settings;
    return { ...result, installCommand: ready ? generatedCommands.commands[index] : "" };
  }), [rawResults, generatedCommands, settings]);
  const [activeTab, setActiveTab] = useState<"graph" | "list">("graph"); const [resultFilter, setResultFilter] = useState<"all" | "found" | "missing" | "error">("all"); const [selectedResults, setSelectedResults] = useState(new Set<string>());
  const failureGroups = useMemo(() => ({ missing: uniquePackages(results.filter(isPlainMissingResult)), timeout: uniquePackages(results.filter((r) => !r.found && r.status === "timeout")), rateLimited: uniquePackages(results.filter((r) => !r.found && r.status === "rateLimited")), error: uniquePackages(results.filter((r) => !r.found && r.status === "error")) }), [results]);
  useEffect(() => { const onKey = (event: KeyboardEvent) => { if (event.altKey && event.key === "1") setActiveTab("graph"); if (event.altKey && event.key === "2") setActiveTab("list"); }; window.addEventListener("keydown", onKey); return () => window.removeEventListener("keydown", onKey); }, []);
  const toggleFilter = useCallback((filter: "found" | "missing" | "error") => setResultFilter((current) => current === filter ? "all" : filter), []);
  const retryFailureGroup = useCallback((label: string, packages: string[]) => { if (!searching && packages.length) { onRetryMissing(packages); onStatusChange(`已回填 ${packages.length} 个${label}包，可重新检索`); } }, [onRetryMissing, onStatusChange, searching]);
  const ReportActions = (props: ComponentProps<typeof ReportActionsBase>) => <ReportActionsBase {...props} settings={settings} />;
  return <div className={`report-layout ${dependencyGraph ? "has-deps" : ""}`}><ReportOverview results={results} packageCount={packageCount} uniqueFoundCount={uniqueFoundCount} searching={searching} searchDuration={searchDuration} stageTimings={stageTimings} smartSuggestions={smartSuggestions} failureGroups={failureGroups} resultFilter={resultFilter} toggleFilter={toggleFilter} retryFailureGroup={retryFailureGroup} onApplySmartSuggestion={onApplySmartSuggestion} /><section className="panel report-panel"><div className="report-panel-header"><PanelHeader step="结果" title="来源验证" meta={searching ? "实时更新" : searchDuration != null ? `已完成 · ${(searchDuration / 1000).toFixed(1)}s` : "已完成"} />{results.length > 0 && <ReportActions results={results} selectedResults={selectedResults} searching={searching} packageCount={packageCount} uniqueFoundCount={uniqueFoundCount} searchDuration={searchDuration} onStatusChange={onStatusChange} onRetryMissing={onRetryMissing} />}</div><ReportResultsTable results={results} searching={searching} resultFilter={resultFilter} setResultFilter={setResultFilter} selectedResults={selectedResults} setSelectedResults={setSelectedResults} onStatusChange={onStatusChange} onCancelPackage={onCancelPackage} onRetryMissing={onRetryMissing} /></section>{dependencyGraph && <section className="panel dependency-panel"><PanelHeader step="扩展" title="依赖关系智能分析" meta={`共构建 ${dependencyGraph.summary.totalNodes} 个包节点，${dependencyGraph.summary.totalEdges} 条依赖链路`} /><div className="dependency-content-wrapper"><div className="tab-header"><button className={`tab-btn ${activeTab === "graph" ? "active" : ""}`} onClick={() => setActiveTab("graph")}>依赖关系图谱<span className="kbd-hint">Alt+1</span></button><button className={`tab-btn ${activeTab === "list" ? "active" : ""}`} onClick={() => setActiveTab("list")}>依赖清单列表<span className="kbd-hint">Alt+2</span></button></div>{activeTab === "graph" ? <DependencyGraphView graph={dependencyGraph} /> : <DependencyListView graph={dependencyGraph} />}</div></section>}<ReportLogs logs={logs} searching={searching} onClearLogs={onClearLogs} onStatusChange={onStatusChange} /></div>;
}
