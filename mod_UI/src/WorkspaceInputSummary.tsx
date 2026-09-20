import type { Ecosystem, InputRules, Settings } from "./types";
import { MAX_INPUT_CHARS, MAX_INPUT_LINE_BYTES, MAX_PACKAGE_LINES, buildSearchPlanPreview, dedupePackageInput, type SmartSuggestion } from "./utils";

interface WorkspaceInputSummaryProps {
  input: string;
  inputTooLarge: boolean;
  inputRules: InputRules;
  inputProfile: { total: number; archiveUrls: number; repositories: number };
  ecosystem: Ecosystem;
  settings: Settings;
  duplicateCount: number;
  smartSuggestions: SmartSuggestion[];
  searching: boolean;
  pasteHint: boolean;
  onInputChange: (value: string, source: "manual" | "clipboard") => string;
  onApplySmartSuggestion: (suggestion: SmartSuggestion) => void;
  onCleanInput: () => void;
  onDismissPasteHint: () => void;
}

export function WorkspaceInputSummary({
  input,
  inputTooLarge,
  inputRules,
  inputProfile,
  ecosystem,
  settings,
  duplicateCount,
  smartSuggestions,
  searching,
  pasteHint,
  onInputChange,
  onApplySmartSuggestion,
  onCleanInput,
  onDismissPasteHint,
}: WorkspaceInputSummaryProps) {
  const searchPlan = buildSearchPlanPreview(inputProfile, { fullSearch: settings.fullSearch, useCache: settings.useCache, duplicateCount });

  return (
    <>
      {inputTooLarge && <div className="inline-warning" id="input-limit-warning" role="alert">输入超出限制或包含非法字符：最多 {MAX_PACKAGE_LINES} 行、总计 {MAX_INPUT_CHARS} 字节、单行 {MAX_INPUT_LINE_BYTES} 字节。</div>}
      {input.length > 0 && (
        <div className="input-stats-bar">
          <span className="input-stat-chip">行数 <strong>{input.split("\n").filter((line) => line.trim()).length}</strong></span>
          <span className="input-stat-chip">字符 <strong>{input.length}</strong></span>
          {inputProfile.total > 0 && (
            <span className="input-stat-chip">
              {ecosystem === "r-binary" ? "待生成" : "CRAN/Bioc"}{" "}
              <strong>{inputProfile.total - inputProfile.archiveUrls - inputProfile.repositories}</strong>
            </span>
          )}
          {inputProfile.repositories > 0 && <span className="input-stat-chip">GitHub <strong>{inputProfile.repositories}</strong></span>}
          {inputProfile.archiveUrls > 0 && <span className="input-stat-chip">URL <strong>{inputProfile.archiveUrls}</strong></span>}
          {duplicateCount > 0 && (
            <button
              type="button"
              className="input-stat-chip warn dedupe-btn"
              title="点击去除重复包名"
              onClick={() => onInputChange(dedupePackageInput(input, inputRules), "manual")}
            >
              重复 <strong>{duplicateCount}</strong> · 去重
            </button>
          )}
        </div>
      )}
      {inputProfile.total > 0 && ecosystem !== "r-binary" && (
        <div className={`search-plan-preview ${searchPlan.level}`} aria-label="搜索计划预览">
          <div><span className="search-plan-eyebrow">搜索计划</span><strong>{searchPlan.summary}</strong><small>{searchPlan.advice}</small></div>
          <div className="search-plan-metrics">
            <span>模式 <strong>{searchPlan.recommendedMode}</strong></span>
            <span>缓存 <strong>{settings.useCache ? "开启" : "关闭"}</strong></span>
            <span>强度 <strong>{searchPlan.level === "heavy" ? "高" : searchPlan.level === "medium" ? "中" : "低"}</strong></span>
          </div>
        </div>
      )}
      {smartSuggestions.length > 0 && (
        <div className="smart-suggestion-list" aria-label="智能建议">
          {smartSuggestions.map((suggestion) => (
            <div className="smart-suggestion" key={suggestion.id}>
              <div><strong>{suggestion.title}</strong><span>{suggestion.detail}</span></div>
              {suggestion.actionLabel && <button type="button" className="text-button" onClick={() => onApplySmartSuggestion(suggestion)} disabled={searching}>{suggestion.actionLabel}</button>}
            </div>
          ))}
        </div>
      )}
      {pasteHint && (
        <div className="paste-hint-bar">
          <span>检测到粘贴内容可能含多余空白、空行或逗号分隔，建议清理后检索</span>
          <div style={{ display: "flex", gap: "6px" }}>
            <button type="button" className="button ghost compact-btn" onClick={() => { onCleanInput(); onDismissPasteHint(); }}>清理</button>
            <button type="button" className="button ghost compact-btn" onClick={onDismissPasteHint}>忽略</button>
          </div>
        </div>
      )}
    </>
  );
}
