import { useEffect, useMemo, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { PanelHeader, EmptyState } from "./components";

export function ReportLogs({ logs, searching, onClearLogs, onStatusChange }: { logs: string[]; searching: boolean; onClearLogs: () => void; onStatusChange: (status: string) => void }) {
  const [query, setQuery] = useState(""); const [wrap, setWrap] = useState(false); const ref = useRef<HTMLDivElement>(null);
  useEffect(() => { if (ref.current) ref.current.scrollTop = ref.current.scrollHeight; }, [logs]);
  const filtered = useMemo(() => query.trim() ? logs.filter((line) => line.toLowerCase().includes(query.trim().toLowerCase())) : logs, [logs, query]);
  return <section className="panel log-panel"><div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}><PanelHeader step="日志" title="检索过程" meta={`${logs.length} 行`} /><div style={{ display: "flex", gap: "8px", alignItems: "center", marginRight: "16px" }}><label className="log-wrap-toggle"><input type="checkbox" checked={wrap} onChange={(e) => setWrap(e.target.checked)} /><span>换行</span></label><input type="text" placeholder="过滤日志..." value={query} onChange={(e) => setQuery(e.target.value)} /><button className="button ghost" onClick={onClearLogs} disabled={searching || logs.length === 0}>清除日志</button></div></div><div className={`log-console${wrap ? " log-wrap" : ""}`} ref={ref}>{filtered.length ? filtered.map((line, index) => <div key={`${line}-${index}`} className="log-line-copyable" onClick={async () => { try { await writeText(line); onStatusChange("已复制日志行"); } catch { /* optional clipboard */ } }}><span>{String(index + 1).padStart(2, "0")}</span>{line}</div>) : <EmptyState text={query ? "无匹配日志" : "日志将在检索开始后显示"} />}</div></section>;
}
