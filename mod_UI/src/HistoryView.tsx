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
          {history.length > 0 && (
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
          )}
        </div>
      </div>
      {history.length === 0 ? (
        <EmptyState text="复制脚本后，命令会记录在这里" />
      ) : (
        <div className="history-list">
          {history
            .filter(record =>
              (record.packageName && record.packageName.toLowerCase().includes(historySearch.toLowerCase())) ||
              (record.toolName && record.toolName.toLowerCase().includes(historySearch.toLowerCase())) ||
              (record.command && record.command.toLowerCase().includes(historySearch.toLowerCase()))
            )
            .map((record) => (
            <article className="history-item" key={record.id}>
              <div className="history-main">
                <div>
                  <strong>{record.packageName || "R 命令"}</strong>
                  <span>{record.toolName}{record.version ? ` · v${record.version}` : ""}</span>
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
