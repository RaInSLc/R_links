import { Metric } from "./components";
import type { SearchResult, SmartSuggestion } from "./utils";
import { isCacheHitResult, isErrorResult, isPlainMissingResult, uniquePackages } from "./reportUtils";

interface Props {
  results: SearchResult[]; packageCount: number; uniqueFoundCount: number; searching: boolean;
  searchDuration: number | null; stageTimings: Array<{ stage: string; durationMs: number }>;
  smartSuggestions: SmartSuggestion[]; failureGroups: Record<"missing" | "timeout" | "rateLimited" | "error", string[]>;
  resultFilter: "all" | "found" | "missing" | "error"; toggleFilter: (filter: "found" | "missing" | "error") => void;
  retryFailureGroup: (label: string, packages: string[]) => void; onApplySmartSuggestion: (suggestion: SmartSuggestion) => void;
}

export function ReportOverview({
  results,
  packageCount,
  uniqueFoundCount,
  searching,
  searchDuration,
  stageTimings,
  smartSuggestions,
  failureGroups,
  resultFilter,
  toggleFilter,
  retryFailureGroup,
  onApplySmartSuggestion,
}: Props) {
  const missingCount = uniquePackages(results.filter(isPlainMissingResult)).length;
  const errorCount = uniquePackages(results.filter(isErrorResult)).length;
  const summary = {
    uniqueResultPackages: uniquePackages(results).length,
    cacheHits: uniquePackages(results.filter(isCacheHitResult)).length,
    failureTotal: Object.values(failureGroups).reduce((sum, items) => sum + items.length, 0),
    verifiedRate: packageCount > 0 ? Math.round((uniqueFoundCount / packageCount) * 100) : 0,
    nextAction: failureGroups.rateLimited.length ? "检测到限流，建议配置 GitHub Token 或稍后重试限流分组。" : failureGroups.timeout.length ? "存在超时包，建议优先重试超时分组。" : failureGroups.error.length ? "存在检索错误，建议检查代理或网络后重试错误分组。" : failureGroups.missing.length ? "存在未找到包，建议开启全量检索或检查包名。" : results.length ? "当前结果稳定，可复制脚本或导出报告。" : "开始检索后将展示任务摘要。",
  };
  const maxStageDuration = Math.max(1, ...stageTimings.map((item) => item.durationMs));
  return <>
    <div className="metric-row">
      <Metric label="输入包" value={packageCount} /><Metric label="已验证包" value={uniqueFoundCount} tone="success" active={resultFilter === "found"} onClick={() => toggleFilter("found")} />
      <Metric label="未找到" value={missingCount} tone="danger" active={resultFilter === "missing"} onClick={() => toggleFilter("missing")} /><Metric label="异常" value={errorCount} tone="warning" active={resultFilter === "error"} onClick={() => toggleFilter("error")} /><Metric label="来源记录" value={results.length} />
    </div>
    {stageTimings.length > 0 && <div className="stage-timing-panel" aria-label="检索耗时分解"><div className="stage-timing-header"><strong>检索耗时分解</strong><small>后端真实阶段计时</small></div>{stageTimings.map((item) => <div className="stage-timing-row" key={item.stage}><span>{item.stage}</span><div className="stage-timing-track"><div className="stage-timing-fill" style={{ width: `${Math.max(2, item.durationMs / maxStageDuration * 100)}%` }} /></div><code>{(item.durationMs / 1000).toFixed(2)}s</code></div>)}</div>}
    {results.length > 0 && <div className="task-summary" aria-label="任务摘要"><div className="task-summary-main"><span className="task-summary-eyebrow">任务摘要</span><strong>{summary.uniqueResultPackages}/{packageCount || summary.uniqueResultPackages} 个包已返回结果 · 验证率 {summary.verifiedRate}%</strong><small>{summary.nextAction}</small></div><div className="task-summary-grid"><span>缓存命中 <strong>{summary.cacheHits}</strong></span><span>失败合计 <strong>{summary.failureTotal}</strong></span><span>超时 <strong>{failureGroups.timeout.length}</strong></span><span>限流 <strong>{failureGroups.rateLimited.length}</strong></span><span>耗时 <strong>{searchDuration != null ? `${(searchDuration / 1000).toFixed(1)}s` : searching ? "进行中" : "-"}</strong></span></div></div>}
    {smartSuggestions.length > 0 && <div className="smart-suggestion-list report-suggestions" aria-label="检索智能建议">{smartSuggestions.map((suggestion) => <div className="smart-suggestion" key={suggestion.id}><div><strong>{suggestion.title}</strong><span>{suggestion.detail}</span></div>{suggestion.actionLabel && <button type="button" className="text-button" onClick={() => onApplySmartSuggestion(suggestion)}>{suggestion.actionLabel}</button>}</div>)}</div>}
    {Object.values(failureGroups).some((items) => items.length > 0) && <div className="failure-breakdown" aria-label="失败分类摘要"><span className="failure-breakdown-title">失败分类</span>{([{ key: "missing", label: "未找到" }, { key: "timeout", label: "超时" }, { key: "rateLimited", label: "限流" }, { key: "error", label: "错误" }] as const).filter((item) => failureGroups[item.key].length).map((item) => <button key={item.key} type="button" className="failure-chip" disabled={searching} onClick={() => retryFailureGroup(item.label, failureGroups[item.key])}> {item.label} <strong>{failureGroups[item.key].length}</strong><small>{searching ? "检索中" : "重试"}</small></button>)}</div>}
  </>;
}
