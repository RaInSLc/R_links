import { useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { EmptyState } from "./components";
import { sourceNames } from "./types";
import type { SearchResult } from "./utils";
import { CACHE_FEEDBACK_SOURCES, getInstallCommand, isErrorResult, isPlainMissingResult, resultSelectionKey, sourceCredibility } from "./reportUtils";

type Filter = "all" | "found" | "missing" | "error";
type SortKey = "package" | "source" | "version" | "status";
interface Props {
  results: SearchResult[]; searching: boolean; resultFilter: Filter; setResultFilter: (filter: Filter) => void;
  selectedResults: Set<string>; setSelectedResults: Dispatch<SetStateAction<Set<string>>>;
  onStatusChange: (status: string) => void; onCancelPackage?: (packageName: string) => void; onRetryMissing: (packages: string[]) => void;
}

const statusRank = (result: SearchResult) => result.found ? 0 : result.status === "timeout" ? 1 : result.status === "rateLimited" ? 2 : result.status === "error" ? 3 : 4;
const pageSources = new Set(["cran", "bioc", "github", "r-forge", "pip", "conda"]);

export function ReportResultsTable({ results, searching, resultFilter, setResultFilter, selectedResults, setSelectedResults, onStatusChange, onCancelPackage, onRetryMissing }: Props) {
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [sourceFilter, setSourceFilter] = useState<string | null>(null);
  const [sortKey, setSortKey] = useState<SortKey>("package");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("asc");
  const [expandedIdentities, setExpandedIdentities] = useState<Set<string>>(new Set());
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [selectedRowIndex, setSelectedRowIndex] = useState(-1);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; result: SearchResult } | null>(null);
  const [compactMode, setCompactMode] = useState(false);
  const [showVersionCol, setShowVersionCol] = useState(true);
  const [showRepoCol, setShowRepoCol] = useState(true);
  const [cacheVotes, setCacheVotes] = useState<Record<string, "up" | "down">>({});
  const [cacheVotePending, setCacheVotePending] = useState<Record<string, boolean>>({});
  const lastChecked = useRef(-1);
  const tableRegion = useRef<HTMLDivElement>(null);
  const [page, setPage] = useState(0);
  const pageSize = 100;

  useEffect(() => { const timer = window.setTimeout(() => setDebouncedQuery(query), 200); return () => window.clearTimeout(timer); }, [query]);
  useEffect(() => { setSelectedRowIndex(-1); setSelectedResults(new Set()); }, [debouncedQuery, resultFilter, sourceFilter, sortKey, sortDir, setSelectedResults]);
  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null); const esc = (event: KeyboardEvent) => { if (event.key === "Escape") close(); };
    window.addEventListener("click", close); window.addEventListener("scroll", close, true); window.addEventListener("keydown", esc);
    return () => { window.removeEventListener("click", close); window.removeEventListener("scroll", close, true); window.removeEventListener("keydown", esc); };
  }, [contextMenu]);

  const filteredResults = useMemo(() => {
    const q = debouncedQuery.trim().toLowerCase();
    return results.filter((result) => {
      const statusMatch = resultFilter === "all" || resultFilter === "found" && result.found || resultFilter === "missing" && isPlainMissingResult(result) || resultFilter === "error" && isErrorResult(result);
      const sourceMatch = !sourceFilter || result.source === sourceFilter;
      return statusMatch && sourceMatch && (!q || result.package.toLowerCase().includes(q) || (result.repository || "").toLowerCase().includes(q));
    });
  }, [results, resultFilter, sourceFilter, debouncedQuery]);
  const allSortedResults = useMemo(() => [...filteredResults].sort((a, b) => {
    const value = sortKey === "version" ? (a.latestVersion || "").localeCompare(b.latestVersion || "", undefined, { numeric: true }) : sortKey === "status" ? statusRank(a) - statusRank(b) : a[sortKey].localeCompare(b[sortKey]);
    return value * (sortDir === "asc" ? 1 : -1);
  }), [filteredResults, sortKey, sortDir]);

  const toggleSort = (key: SortKey) => { if (sortKey === key) setSortDir((dir) => dir === "asc" ? "desc" : "asc"); else { setSortKey(key); setSortDir("asc"); } };
  useEffect(() => { setPage(0); lastChecked.current = -1; setSelectedRowIndex(-1); }, [debouncedQuery, resultFilter, sourceFilter, sortKey, sortDir]);
  const pageCount = Math.max(1, Math.ceil(allSortedResults.length / pageSize));
  const currentPage = Math.min(page, pageCount - 1);
  const sortedResults = useMemo(() => allSortedResults.slice(currentPage * pageSize, (currentPage + 1) * pageSize), [allSortedResults, currentPage]);
  const rowIdentities = new Map(sortedResults.map((result, index) => [`${result.package}-${result.source}-${index}`, resultSelectionKey(result)]));
  const expanded = new Set([...rowIdentities].filter(([, identity]) => expandedIdentities.has(identity)).map(([key]) => key));
  const setExpanded = (keys: Set<string>) => setExpandedIdentities(new Set([...keys].map((key) => rowIdentities.get(key)).filter((key): key is string => key !== undefined)));
  useEffect(() => {
    const region = tableRegion.current;
    if (!region) return;
    const elements = region.querySelectorAll<HTMLElement>(".pkg-link, .source-tag, .version-copyable, .repo-clickable, .status-clickable");
    elements.forEach((element) => { element.tabIndex = 0; element.setAttribute("role", "button"); });
    const activate = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if ((event.key === "Enter" || event.key === " ") && target.matches(".pkg-link, .source-tag, .version-copyable, .repo-clickable, .status-clickable")) { event.preventDefault(); event.stopPropagation(); target.click(); }
    };
    const handleClick = (event: MouseEvent) => {
      const target = event.target as HTMLElement;
      if (!target.matches(".version-copyable")) return;
      event.preventDefault(); event.stopPropagation();
      void writeText(target.textContent?.split("（请求")[0] || "").catch((error) => onStatusChange(`复制版本失败: ${String(error)}`));
    };
    region.addEventListener("keydown", activate);
    region.addEventListener("click", handleClick, true);
    return () => { region.removeEventListener("keydown", activate); region.removeEventListener("click", handleClick, true); };
  }, [allSortedResults, currentPage, showRepoCol, showVersionCol, onStatusChange]);
  useEffect(() => { lastChecked.current = -1; }, [currentPage, debouncedQuery, resultFilter, sourceFilter, sortKey, sortDir]);
  const toggleExpanded = (key: string) => { const identity = rowIdentities.get(key); if (!identity) return; setExpandedIdentities((current) => { const next = new Set(current); next.has(identity) ? next.delete(identity) : next.add(identity); return next; }); };
  const handleCopy = async (result: SearchResult, key: string) => { try { const command = getInstallCommand(result); if (!command) { onStatusChange("安装命令尚未生成，请稍后重试"); return; } await writeText(command); setCopiedKey(key); onStatusChange(result.found ? `已复制 ${result.package} 的安装指令` : `已复制包名 ${result.package}`); window.setTimeout(() => setCopiedKey(null), 1500); } catch (error) { onStatusChange(`复制安装指令失败: ${error instanceof Error ? error.message : String(error)}`); } };
  const handleOpenPage = async (result: SearchResult) => { if (!result.found || !pageSources.has(result.source)) return; try { await invoke("open_package_page", { package: result.realName || result.package, source: result.source, repository: result.repository || "" }); onStatusChange(`已打开 ${result.package} 的来源网页`); } catch (error) { onStatusChange(`打开来源网页失败: ${error instanceof Error ? error.message : String(error)}`); } };
  const handleRateCache = async (result: SearchResult, vote: "up" | "down") => {
    if (!result.found) return;
    if (searching) { onStatusChange("检索完成后才能反馈缓存结果"); return; }
    const key = `${result.package}\u0001${result.source}\u0001${result.repository}\u0001${result.realName}`;
    if (cacheVotePending[key]) return;
    setCacheVotePending((current) => ({ ...current, [key]: true }));
    try { const message = await invoke<string>("rate_cache_result", { package: result.package, source: result.source, version: result.latestVersion || "", repository: result.repository || "", realName: result.realName || result.package, vote }); setCacheVotes((current) => ({ ...current, [key]: vote })); onStatusChange(message); }
    catch (error) { onStatusChange(`缓存反馈失败: ${error instanceof Error ? error.message : String(error)}`); }
    finally { setCacheVotePending((current) => ({ ...current, [key]: false })); }
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (!tableRegion.current?.contains(target) || target.closest("input, textarea, button, a, select, [contenteditable=true]") || sortedResults.length === 0) return;
      const first = 0;
      const last = sortedResults.length - 1;
      if (event.key === "ArrowDown") { event.preventDefault(); setSelectedRowIndex((index) => Math.min(Math.max(index + 1, first), last)); }
      else if (event.key === "ArrowUp") { event.preventDefault(); setSelectedRowIndex((index) => Math.max(Math.min(index - 1, last), first)); }
      else if (selectedRowIndex >= first && selectedRowIndex <= last) {
        const result = sortedResults[selectedRowIndex];
        if (event.key === "Enter") { event.preventDefault(); void handleCopy(result, resultSelectionKey(result)); }
        else if (event.key === " ") { event.preventDefault(); toggleExpanded(`${result.package}-${result.source}-${selectedRowIndex}`); }
      }
    };
    window.addEventListener("keydown", onKey); return () => window.removeEventListener("keydown", onKey);
  });

  if (!results.length) return <EmptyState text={searching ? "正在等待首条检索结果" : "尚未执行检索"} hint={searching ? undefined : "请在「工作台」页面输入包名并点击「开始检索」"} />;
  const selectAll = sortedResults.length > 0 && sortedResults.every((result) => selectedResults.has(resultSelectionKey(result)));
  return <div ref={tableRegion} tabIndex={0} aria-label="结果表格操作区域">
    {pageCount > 1 && <div className="result-filter-bar"><button type="button" disabled={currentPage === 0} onClick={() => { setPage(currentPage - 1); setSelectedRowIndex(-1); }}>上一页</button><span>第 {currentPage + 1} / {pageCount} 页，每页 {pageSize} 条</span><button type="button" disabled={currentPage + 1 >= pageCount} onClick={() => { setPage(currentPage + 1); setSelectedRowIndex(-1); }}>下一页</button></div>}
    <div className="result-filter-bar"><div className="result-filter-tabs">{(["all", "found", "missing", "error"] as const).map((key) => <button key={key} type="button" className={`result-filter-tab ${resultFilter === key ? "active" : ""}`} onClick={() => setResultFilter(key)}>{key === "all" ? `全部 ${results.length}` : key === "found" ? `已验证 ${results.filter((r) => r.found).length}` : key === "missing" ? `未找到 ${results.filter(isPlainMissingResult).length}` : `异常 ${results.filter(isErrorResult).length}`}</button>)}</div><input className="result-search-input" placeholder="筛选包名或仓库..." value={query} onChange={(event) => setQuery(event.target.value)} />{query && <button type="button" className="search-clear-btn" onClick={() => setQuery("")}>✕</button>}{(debouncedQuery || resultFilter !== "all" || sourceFilter) && <small className="result-count-hint">显示 {filteredResults.length}/{results.length} 条 {sourceFilter && <button type="button" className="source-filter-clear" onClick={() => setSourceFilter(null)}>{sourceNames[sourceFilter] ?? sourceFilter} ✕</button>}</small>}<button type="button" className={`button ghost compact-btn${compactMode ? " active" : ""}`} onClick={() => setCompactMode((value) => !value)}>紧凑{compactMode ? "✓" : ""}</button><button type="button" className={`button ghost compact-btn${showVersionCol ? " active" : ""}`} onClick={() => setShowVersionCol((value) => !value)}>版本{showVersionCol ? "✓" : "✕"}</button><button type="button" className={`button ghost compact-btn${showRepoCol ? " active" : ""}`} onClick={() => setShowRepoCol((value) => !value)}>仓库{showRepoCol ? "✓" : "✕"}</button><button type="button" className="button ghost compact-btn" onClick={() => setExpanded(new Set(sortedResults.map((r, i) => `${r.package}-${r.source}-${i}`)))}>全部展开</button><button type="button" className="button ghost compact-btn" onClick={() => setExpanded(new Set())}>全部收起</button></div>
    <div className="status-legend"><span className="legend-item"><span className="legend-dot found" />已验证</span><span className="legend-item"><span className="legend-dot missing" />未找到</span><span className="legend-item"><span className="legend-dot timeout" />超时</span><span className="legend-item"><span className="legend-dot rate-limited" />频率限制</span><span className="legend-item"><span className="legend-dot error" />异常</span></div>
    {!sortedResults.length ? <EmptyState text="当前筛选条件下无匹配结果" hint="尝试切换上方的筛选标签或清空搜索框" /> : <div className="result-table-wrapper"><div className={`result-table${compactMode ? " compact" : ""}${showVersionCol ? "" : " hide-version"}${showRepoCol ? "" : " hide-repo"}`} role="table" aria-label="包来源验证结果"><div className="result-row result-head" role="row"><span>#</span><span><input type="checkbox" checked={selectAll} onChange={() => setSelectedResults(selectAll ? new Set() : new Set(sortedResults.map(resultSelectionKey)))} aria-label="全选" /></span>{(["package", "source", ...(showVersionCol ? ["version"] : []), "status"] as SortKey[]).map((key) => <button key={key} type="button" className={`sortable ${sortKey === key ? `sorted-${sortDir}` : ""}`} onClick={() => toggleSort(key)}>{key === "package" ? "包名" : key === "source" ? "来源" : key === "version" ? "版本" : "状态"}</button>)}{showRepoCol && <span>仓库</span>}</div>
      {sortedResults.map((result, index) => { const rowKey = `${result.package}-${result.source}-${index}`; const selectionKey = resultSelectionKey(result); const feedbackKey = `${result.package}\u0001${result.source}\u0001${result.repository}\u0001${result.realName}`; const isExpanded = expanded.has(rowKey); const status = result.found ? "found" : result.status === "timeout" ? "timeout" : result.status === "rateLimited" ? "rate-limited" : result.status === "error" ? "error" : "missing"; return <div key={rowKey}><div className={`result-row${selectedRowIndex === index ? " row-selected" : ""}`} role="row" onMouseEnter={() => setSelectedRowIndex(index)} onDoubleClick={() => toggleExpanded(rowKey)} onContextMenu={(event) => { event.preventDefault(); setContextMenu({ x: event.clientX, y: event.clientY, result }); }}><span>{index + 1}</span><span><input type="checkbox" checked={selectedResults.has(selectionKey)} onChange={() => {}} onClick={(event) => { const next = new Set(selectedResults); if (event.shiftKey && lastChecked.current >= 0) for (let i = Math.min(lastChecked.current, index); i <= Math.max(lastChecked.current, index); i++) next.add(resultSelectionKey(sortedResults[i])); else next.has(selectionKey) ? next.delete(selectionKey) : next.add(selectionKey); setSelectedResults(next); lastChecked.current = index; }} aria-label={`选择 ${result.package}`} /></span><strong className={result.found && pageSources.has(result.source) ? "pkg-link" : ""} onClick={(event) => event.altKey ? setQuery(result.package) : void handleOpenPage(result)} title="点击打开网页 · Alt+点击搜索此包">{result.package}</strong>{searching && onCancelPackage && <button type="button" className="row-copy-btn" onClick={() => onCancelPackage(result.package)}>取消</button>}<span className="source-cell-with-copy"><span className={`source-tag ${result.source}${sourceFilter === result.source ? " tag-active" : ""}`} onClick={() => setSourceFilter(sourceFilter === result.source ? null : result.source)}>{sourceNames[result.source] ?? result.source}</span><small>{sourceCredibility(result.source, result.stage)}</small><button type="button" className={`row-copy-btn ${copiedKey === rowKey ? "copied" : ""}`} onClick={() => void handleCopy(result, rowKey)} title={result.found ? `复制安装指令: ${getInstallCommand(result)}` : `复制包名: ${result.package}`}>复制</button></span>{showVersionCol && <code className={result.latestVersion ? "version-copyable" : ""} onClick={() => result.latestVersion && void writeText(result.latestVersion)}>{result.requestedVersion && result.latestVersion && result.requestedVersion !== result.latestVersion ? `${result.latestVersion}（请求 ${result.requestedVersion}）` : result.latestVersion || "—"}</code>}{showRepoCol && <span className={result.found && result.repository ? "repo-clickable" : ""} onClick={() => result.found && result.repository && void handleOpenPage(result)}>{result.repository || "—"}</span>}<span className={`${status} status-clickable`} onClick={() => setResultFilter(status === "found" ? "found" : status === "missing" ? "missing" : "error")}><span>{status === "timeout" ? "超时" : status === "rate-limited" ? "频率限制" : status === "error" ? "检索异常" : status === "found" ? "已验证" : "未找到"}</span>{result.found && CACHE_FEEDBACK_SOURCES.has(result.source) && <span className="cache-feedback" onClick={(event) => event.stopPropagation()}><button type="button" className={`cache-feedback-btn ${cacheVotes[feedbackKey] === "up" ? "active" : ""}`} disabled={searching || cacheVotePending[feedbackKey]} onClick={() => void handleRateCache(result, "up")}>赞</button><button type="button" className={`cache-feedback-btn ${cacheVotes[feedbackKey] === "down" ? "active bad" : ""}`} disabled={searching || cacheVotePending[feedbackKey]} onClick={() => void handleRateCache(result, "down")}>踩</button></span>}</span><button type="button" className="row-expand-btn" onClick={() => toggleExpanded(rowKey)}>{isExpanded ? "▴" : "▾"}</button></div>{isExpanded && <div className="result-detail"><div className="result-detail-grid"><div><span className="detail-label">安装命令</span><code className="detail-code">{getInstallCommand(result)}</code></div>{result.realName && result.realName !== result.package && <div><span className="detail-label">规范名称</span><span>{result.realName}</span></div>}<div><span className="detail-label">仓库地址</span><span>{result.repository || "—"}</span></div><div><span className="detail-label">检索状态</span><span>{status === "found" ? "已验证" : status === "timeout" ? "超时" : status === "rate-limited" ? "频率限制" : status === "error" ? "检索异常" : "未找到"}</span></div></div></div>}</div>; })}<div className="result-table-footer">共 {filteredResults.length} 条 · 已验证 {filteredResults.filter((r) => r.found).length} · 未找到 {filteredResults.filter(isPlainMissingResult).length} · 异常 {filteredResults.filter(isErrorResult).length}{selectedResults.size > 0 && <span className="footer-selected"> · 已选中 {selectedResults.size}</span>}</div></div></div>}
    {contextMenu && <div className="ctx-menu" style={{ position: "fixed", left: contextMenu.x, top: contextMenu.y, zIndex: 1000 }} onClick={(event) => event.stopPropagation()}><button type="button" className="ctx-menu-item" onClick={() => { void handleCopy(contextMenu.result, `${contextMenu.result.package}-ctx`); setContextMenu(null); }}>{contextMenu.result.found ? "复制安装命令" : "复制包名"}</button>{contextMenu.result.found && pageSources.has(contextMenu.result.source) && <button type="button" className="ctx-menu-item" onClick={() => { void handleOpenPage(contextMenu.result); setContextMenu(null); }}>打开来源网页</button>}{!contextMenu.result.found && <button type="button" className="ctx-menu-item" onClick={() => { onRetryMissing([contextMenu.result.package]); setContextMenu(null); }}>重试此包</button>}<button type="button" className="ctx-menu-item" onClick={() => { const index = sortedResults.indexOf(contextMenu.result); toggleExpanded(`${contextMenu.result.package}-${contextMenu.result.source}-${index}`); setContextMenu(null); }}>展开/收起详情</button>{contextMenu.result.latestVersion && <button type="button" className="ctx-menu-item" onClick={() => { void writeText(contextMenu.result.latestVersion || ""); setContextMenu(null); }}>复制版本号</button>}</div>}
  </div>;
}
