import { useState } from "react";
import { PanelHeader, EmptyState } from "./components";
import type { HistoryRecord } from "./utils";

interface HistoryViewProps {
  history: HistoryRecord[];
  historySearch: string;
  onHistorySearchChange: (value: string) => void;
  onApplyRecord: (record: HistoryRecord) => void;
  onCopyRecord: (record: HistoryRecord) => void;
  onDeleteRecord: (id: string) => void;
  onClearAll: () => void;
}

export function HistoryView({
  history, historySearch, onHistorySearchChange,
  onApplyRecord, onCopyRecord, onDeleteRecord, onClearAll,
}: HistoryViewProps) {
  const [sortBy, setSortBy] = useState<"time" | "name">("time");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  const filtered = history
    .filter(record =>
      (record.packageName && record.packageName.toLowerCase().includes(historySearch.toLowerCase())) ||
      (record.toolName && record.toolName.toLowerCase().includes(historySearch.toLowerCase())) ||
      (record.command && record.command.toLowerCase().includes(historySearch.toLowerCase()))
    );

  const sorted = sortBy === "name"
    ? [...filtered].sort((a, b) => (a.packageName || "").localeCompare(b.packageName || ""))
    : filtered;

  const toggleSelect = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const allSelected = sorted.length > 0 && sorted.every((r) => selectedIds.has(r.id));
  const toggleSelectAll = () => {
    if (allSelected) setSelectedIds(new Set());
    else setSelectedIds(new Set(sorted.map((r) => r.id)));
  };

  const deleteSelected = () => {
    if (window.confirm(`确定删除选中的 ${selectedIds.size} 条记录？`)) {
      selectedIds.forEach((id) => onDeleteRecord(id));
      setSelectedIds(new Set());
    }
  };

  return (
    <section className="panel history-panel">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <PanelHeader step="历史" title="最近生成的命令" meta="最多保留 100 条" />
        <div style={{ display: "flex", gap: "8px", alignItems: "center", marginRight: "16px" }}>
          <input
            type="text"
            placeholder="搜索包名、来源或命令..."
            value={historySearch}
            onChange={(e) => onHistorySearchChange(e.target.value)}
            style={{ padding: "4px 8px", fontSize: "12px", width: "200px" }}
          />
          {filtered.length > 0 && (
            <button
              type="button"
              className="button ghost"
              style={{ padding: "4px 10px", fontSize: "11px", height: "30px", minHeight: "auto", whiteSpace: "nowrap" }}
              onClick={() => setSortBy(sortBy === "time" ? "name" : "time")}
              title={sortBy === "time" ? "当前：按时间排序，点击切换为按名称" : "当前：按名称排序，点击切换为按时间"}
            >
              {sortBy === "time" ? "时间↓" : "名称A-Z"}
            </button>
          )}
          {history.length > 0 && (
            <>
              {selectedIds.size > 0 && (
                <button
                  type="button"
                  className="button ghost danger-text"
                  style={{ padding: "4px 10px", fontSize: "11px", height: "30px", minHeight: "auto", whiteSpace: "nowrap" }}
                  onClick={deleteSelected}
                >
                  删除选中({selectedIds.size})
                </button>
              )}
              <button
                type="button"
                className="button ghost"
                style={{ padding: "4px 10px", fontSize: "11px", height: "30px", minHeight: "auto", whiteSpace: "nowrap" }}
                title="复制当前筛选结果的全部命令"
                onClick={async () => {
                  const cmds = sorted.map((r) => r.command).filter(Boolean);
                  if (cmds.length === 0) return;
                  try {
                    await navigator.clipboard.writeText(cmds.join("\n"));
                  } catch { /* ignore */ }
                }}
              >
                复制全部
              </button>
              <button
                type="button"
                className="button ghost"
                style={{ padding: "4px 10px", fontSize: "11px", height: "30px", minHeight: "auto", whiteSpace: "nowrap" }}
                onClick={() => {
                  if (window.confirm(`确定清空全部 ${history.length} 条历史记录？此操作不可撤销。`)) {
                    onClearAll();
                  }
                }}
              >
                清空全部
              </button>
            </>
          )}
        </div>
      </div>
      {history.length === 0 ? (
        <EmptyState text="复制脚本后，命令会记录在这里" />
      ) : sorted.length === 0 ? (
        <EmptyState text="无匹配的历史记录" hint="尝试修改搜索关键词" />
      ) : (
        <div className="history-list">
          <div className="history-select-all-row">
            <label className="history-checkbox-label">
              <input
                type="checkbox"
                checked={allSelected}
                onChange={toggleSelectAll}
              />
              全选
            </label>
          </div>
          {sorted.map((record) => (
            <article className={`history-item ${selectedIds.has(record.id) ? "selected" : ""}`} key={record.id}>
              <input
                type="checkbox"
                className="history-checkbox"
                checked={selectedIds.has(record.id)}
                onChange={() => toggleSelect(record.id)}
              />
              <div className="history-main">
                <div>
                  <strong>{record.packageName || "R 命令"}</strong>
                  <span>{record.toolName}{record.version ? ` · v${record.version}` : ""}</span>
                  {record.createdAt && (
                    <span className="history-time">{new Date(record.createdAt).toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" })}</span>
                  )}
                </div>
                <code>{record.command}</code>
              </div>
              <div className="history-actions">
                <button className="text-button" onClick={() => onApplyRecord(record)}>应用</button>
                <button className="text-button" onClick={() => onCopyRecord(record)}>复制</button>
                <button className="text-button danger-text" onClick={() => onDeleteRecord(record.id)}>删除</button>
              </div>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
