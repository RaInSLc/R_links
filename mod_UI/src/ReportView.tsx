import { useState, useMemo, useRef, useCallback, useEffect, Fragment } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PanelHeader, Metric, EmptyState } from "./components";
import { sourceNames } from "./types";
import type { SearchResult, DependencyGraph, DependencyNode, ReverseDependenciesInfo, SmartSuggestion } from "./utils";

interface ReportViewProps {
  results: SearchResult[];
  logs: string[];
  dependencyGraph: DependencyGraph | null;
  packageCount: number;
  uniqueFoundCount: number;
  smartSuggestions: SmartSuggestion[];
  searching: boolean;
  searchDuration: number | null;
  onClearLogs: () => void;
  onStatusChange: (status: string) => void;
  onApplySmartSuggestion: (suggestion: SmartSuggestion) => void;
  onRetryMissing: (packages: string[]) => void;
}

function DependencyGraphView({ graph }: { graph: DependencyGraph }) {
  const [hoveredNode, setHoveredNode] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<DependencyNode | null>(null);
  const [filterStrength, setFilterStrength] = useState<"all" | "heavy">("all");
  const [depSearch, setDepSearch] = useState("");
  const [reverseDeps, setReverseDeps] = useState<ReverseDependenciesInfo | null>(null);
  const [reverseDepsLoading, setReverseDepsLoading] = useState(false);
  const fetchDepsToken = useRef(0);

  async function fetchReverseDeps(packageName: string) {
    const token = ++fetchDepsToken.current;
    setReverseDepsLoading(true);
    setReverseDeps(null);
    try {
      const info = await invoke<ReverseDependenciesInfo>("fetch_reverse_dependencies", {
        packageName,
        mirror: "",
      });
      if (token !== fetchDepsToken.current) return;
      setReverseDeps(info);
    } catch {
      if (token !== fetchDepsToken.current) return;
      setReverseDeps(null);
    } finally {
      if (token === fetchDepsToken.current) {
        setReverseDepsLoading(false);
      }
    }
  }

  const filteredEdges = useMemo(() => {
    if (filterStrength === "heavy") {
      return graph.edges.filter((e) => e.strength === "heavy");
    }
    return graph.edges;
  }, [graph.edges, filterStrength]);

  const filteredNodes = useMemo(() => {
    if (filterStrength === "heavy") {
      const activePackages = new Set<string>();
      graph.roots.forEach((r) => activePackages.add(r));
      filteredEdges.forEach((e) => {
        activePackages.add(e.from);
        activePackages.add(e.to);
      });
      return graph.nodes.filter((n) => activePackages.has(n.package));
    }
    return graph.nodes;
  }, [graph.nodes, filteredEdges, filterStrength]);

  const { layoutNodes, svgWidth, svgHeight } = useMemo(() => {
    const levels: Record<number, string[]> = {};
    filteredNodes.forEach((n) => {
      const d = n.depth;
      if (!levels[d]) levels[d] = [];
      levels[d].push(n.package);
    });

    const maxDepth = Math.max(...filteredNodes.map((n) => n.depth), 0);
    const colWidth = 220;
    const svgWidth = Math.max(700, (maxDepth + 1) * colWidth + 100);

    const levelCounts = Object.values(levels).map((arr) => arr.length);
    const maxNodesInLevel = Math.max(...levelCounts, 1);
    const nodeSpacing = 48;
    const svgHeight = Math.max(450, maxNodesInLevel * nodeSpacing + 60);

    const positions: Record<string, { x: number; y: number }> = {};

    Object.keys(levels).forEach((depthStr) => {
      const depth = Number(depthStr);
      const pkgs = levels[depth];
      const count = pkgs.length;

      pkgs.forEach((pkg, index) => {
        const x = 50 + depth * colWidth;
        const y =
          count === 1
            ? svgHeight / 2
            : (svgHeight - (count - 1) * nodeSpacing) / 2 + index * nodeSpacing;
        positions[pkg] = { x, y };
      });
    });

    return { layoutNodes: positions, svgWidth, svgHeight };
  }, [filteredNodes]);

  const highlightedNodes = useMemo(() => {
    if (!hoveredNode) return null;
    const set = new Set<string>([hoveredNode]);
    filteredEdges.forEach((e) => {
      if (e.from === hoveredNode) set.add(e.to);
      if (e.to === hoveredNode) set.add(e.from);
    });
    return set;
  }, [hoveredNode, filteredEdges]);

  return (
    <div className="dep-graph-container">
      <div className="dep-graph-toolbar" style={{ display: "flex", alignItems: "center", marginBottom: "12px" }}>
        <span className="toolbar-title" style={{ fontSize: "13px", fontWeight: "500", marginRight: "12px" }}>图谱过滤：</span>
        <button
          className={`button small ${filterStrength === "all" ? "primary" : "ghost"}`}
          onClick={() => setFilterStrength("all")}
          style={{ padding: "4px 10px", fontSize: "12px", height: "auto" }}
        >
          显示所有依赖 ({graph.summary.totalNodes})
        </button>
        <button
          className={`button small ${filterStrength === "heavy" ? "primary" : "ghost"}`}
          onClick={() => setFilterStrength("heavy")}
          style={{ padding: "4px 10px", fontSize: "12px", height: "auto", marginLeft: "8px" }}
        >
          仅重度依赖 ({graph.summary.heavyNodes})
        </button>
        <input
          type="text"
          value={depSearch}
          onChange={(e) => setDepSearch(e.target.value)}
          placeholder="搜索节点..."
          style={{ marginLeft: "auto", padding: "4px 8px", fontSize: "12px", width: "140px", borderRadius: "4px", border: "1px solid var(--line)", background: "var(--input-bg, #fff)", color: "var(--ink)" }}
        />
      </div>

      <div className="dep-graph-workspace" style={{ display: "flex", gap: "16px" }}>
        <div
          className="dep-graph-viewport"
          style={{
            flex: 1,
            height: "480px",
            overflow: "auto",
            border: "1px solid var(--border-color, #e0e0e0)",
            borderRadius: "6px",
            backgroundColor: "var(--console-bg, #fafafa)",
            position: "relative",
          }}
        >
          <svg width={svgWidth} height={svgHeight} style={{ overflow: "visible" }}>
            <defs>
              <marker
                id="arrow-heavy"
                viewBox="0 0 10 10"
                refX="12"
                refY="5"
                markerWidth="6"
                markerHeight="6"
                orient="auto-start-reverse"
              >
                <path d="M 0 0 L 10 5 L 0 10 z" fill="#0066cc" />
              </marker>
              <marker
                id="arrow-light"
                viewBox="0 0 10 10"
                refX="12"
                refY="5"
                markerWidth="6"
                markerHeight="6"
                orient="auto-start-reverse"
              >
                <path d="M 0 0 L 10 5 L 0 10 z" fill="#b0b0b0" />
              </marker>
            </defs>

            {filteredEdges.map((edge, idx) => {
              const fromPos = layoutNodes[edge.from];
              const toPos = layoutNodes[edge.to];
              if (!fromPos || !toPos) return null;

              const isRelatedToHover = hoveredNode
                ? edge.from === hoveredNode || edge.to === hoveredNode
                : false;

              const strokeColor = edge.strength === "heavy"
                ? (hoveredNode ? (isRelatedToHover ? "#0066cc" : "#e0e0e0") : "#a3c2e0")
                : (hoveredNode ? (isRelatedToHover ? "#b0b0b0" : "#f0f0f0") : "#d0d0d0");

              const strokeDash = edge.strength === "light" ? "3,3" : undefined;
              const strokeWidth = edge.strength === "heavy" ? (isRelatedToHover ? 2.5 : 1.5) : 1.0;
              const opacity = hoveredNode ? (isRelatedToHover ? 1.0 : 0.2) : 0.8;

              const dx = toPos.x - fromPos.x;
              const cx1 = fromPos.x + dx / 2;
              const cy1 = fromPos.y;
              const cx2 = fromPos.x + dx / 2;
              const cy2 = toPos.y;

              return (
                <path
                  key={`edge-${idx}`}
                  d={`M ${fromPos.x + 130} ${fromPos.y} C ${cx1 + 65} ${cy1}, ${cx2 + 65} ${cy2}, ${toPos.x} ${toPos.y}`}
                  fill="none"
                  stroke={strokeColor}
                  strokeWidth={strokeWidth}
                  strokeDasharray={strokeDash}
                  opacity={opacity}
                  markerEnd={edge.strength === "heavy" ? "url(#arrow-heavy)" : "url(#arrow-light)"}
                  style={{ transition: "stroke 0.2s, stroke-width 0.2s, opacity 0.2s" }}
                />
              );
            })}

            {filteredNodes.map((node) => {
              const pos = layoutNodes[node.package];
              if (!pos) return null;

              const isHighlighted = highlightedNodes ? highlightedNodes.has(node.package) : true;
              const isHovered = hoveredNode === node.package;
              const opacity = hoveredNode ? (isHighlighted ? 1.0 : 0.3) : 1.0;
              const matchesSearch = !depSearch.trim() || node.package.toLowerCase().includes(depSearch.trim().toLowerCase());

              const isRoot = graph.roots.includes(node.package);
              const isShared = node.rootPackages.length > 1;

              let borderClass = "dep-node-normal";
              if (isRoot) borderClass = "dep-node-root";
              else if (isShared) borderClass = "dep-node-shared";
              else if (node.status === "unresolved") borderClass = "dep-node-unresolved";

              return (
                <foreignObject
                  key={`node-${node.package}`}
                  x={pos.x}
                  y={pos.y - 18}
                  width="150"
                  height="36"
                  style={{ overflow: "visible", cursor: "pointer", transition: "opacity 0.2s", opacity: matchesSearch ? opacity : 0.15 }}
                  onMouseEnter={() => setHoveredNode(node.package)}
                  onMouseLeave={() => setHoveredNode(null)}
                  onClick={() => { setSelectedNode(node); fetchReverseDeps(node.package); }}
                >
                  <div className={`dep-node-card ${borderClass} ${isHovered ? "hovered" : ""}`}>
                    <span className="dep-node-title" title={node.package}>
                      {node.package}
                    </span>
                    <span className="dep-node-meta">
                      {isRoot ? "根依赖包" : node.version !== "unknown" ? `v${node.version}` : "解析失败"}
                    </span>
                    {isShared && <div className="dep-node-shared-badge" title="多根共享依赖">S</div>}
                  </div>
                </foreignObject>
              );
            })}
          </svg>
        </div>

        <div
          className="dep-graph-sidebar"
          style={{
            width: "240px",
            border: "1px solid var(--border-color, #e0e0e0)",
            borderRadius: "6px",
            padding: "16px",
            backgroundColor: "var(--card-bg, #ffffff)",
            fontSize: "13px",
          }}
        >
          {selectedNode ? (
            <div>
              <h4 style={{ margin: "0 0 12px 0", color: "var(--primary-color)" }}>{selectedNode.package}</h4>
              <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                <div><strong>来源：</strong><span className={`source-tag ${selectedNode.source}`}>{sourceNames[selectedNode.source] ?? selectedNode.source}</span></div>
                <div><strong>版本：</strong><code>{selectedNode.version}</code></div>
                <div><strong>引入深度：</strong><code>{selectedNode.depth}</code></div>
                <div>
                  <strong>根包路径：</strong>
                  <div style={{ display: "flex", flexWrap: "wrap", gap: "4px", marginTop: "4px" }}>
                    {selectedNode.rootPackages.map((r) => (
                      <span key={r} style={{ padding: "2px 6px", borderRadius: "3px", backgroundColor: "#f0f0f0", fontSize: "11px" }}>{r}</span>
                    ))}
                  </div>
                </div>
                <div><strong>子依赖数：</strong><code>{selectedNode.directDependencyCount}</code></div>
                <div><strong>重子依赖：</strong><code>{selectedNode.heavyDependencyCount}</code></div>
                <div><strong>解析状态：</strong><span style={{ color: selectedNode.status === "resolved" ? "green" : "red" }}>{selectedNode.status === "resolved" ? "已解析" : "解析失败"}</span></div>
              </div>
              <div style={{ marginTop: "12px", borderTop: "1px solid var(--line)", paddingTop: "10px" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "8px" }}>
                  <strong style={{ fontSize: "12px" }}>反向依赖分析 (CRAN)</strong>
                  <button
                    className="button ghost"
                    style={{ padding: "2px 8px", fontSize: "11px", height: "auto", minHeight: "auto" }}
                    onClick={() => fetchReverseDeps(selectedNode.package)}
                    disabled={reverseDepsLoading}
                  >
                    {reverseDepsLoading ? "加载中..." : "刷新"}
                  </button>
                </div>
                {reverseDeps ? (
                  <div style={{ display: "flex", flexDirection: "column", gap: "4px", fontSize: "12px" }}>
                    <div><span style={{ color: "var(--muted)" }}>反向 Depends:</span> <strong>{reverseDeps.depends}</strong></div>
                    <div><span style={{ color: "var(--muted)" }}>反向 Imports:</span> <strong>{reverseDeps.imports}</strong></div>
                    <div><span style={{ color: "var(--muted)" }}>反向 Suggests:</span> <strong>{reverseDeps.suggests}</strong></div>
                    <div><span style={{ color: "var(--muted)" }}>反向 LinkingTo:</span> <strong>{reverseDeps.linkingTo}</strong></div>
                  </div>
                ) : reverseDepsLoading ? (
                  <span style={{ color: "var(--muted)", fontSize: "12px" }}>正在查询 CRAN...</span>
                ) : (
                  <span style={{ color: "var(--muted)", fontSize: "12px" }}>非 CRAN 包或查询失败</span>
                )}
              </div>
            </div>
          ) : (
            <div style={{ color: "#999", textAlign: "center", paddingTop: "60px" }}>
              点击图谱节点查看详细信息
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function DependencyListView({ graph }: { graph: DependencyGraph }) {
  const [searchTerm, setSearchTerm] = useState("");
  const [filterType, setFilterType] = useState<"all" | "heavy" | "light" | "shared">("all");

  const filteredNodes = useMemo(() => {
    return graph.nodes.filter((node) => {
      const matchSearch = node.package.toLowerCase().includes(searchTerm.toLowerCase());
      const isShared = node.rootPackages.length > 1;
      const isLight =
        graph.edges.some((e) => e.to === node.package && e.strength === "light") &&
        !graph.roots.includes(node.package);
      const isHeavy = !isLight;

      if (filterType === "heavy") return matchSearch && isHeavy;
      if (filterType === "light") return matchSearch && isLight;
      if (filterType === "shared") return matchSearch && isShared;
      return matchSearch;
    });
  }, [graph.nodes, graph.edges, searchTerm, filterType]);

  return (
    <div className="dep-list-container">
      <div
        className="dep-list-filters"
        style={{
          display: "flex",
          gap: "8px",
          marginBottom: "16px",
          alignItems: "center",
          flexWrap: "wrap",
        }}
      >
        <input
          type="text"
          placeholder="搜索依赖包名..."
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          className="input"
          style={{ width: "180px", padding: "4px 8px", fontSize: "13px" }}
        />
        <button
          className={`button small ${filterType === "all" ? "primary" : "ghost"}`}
          onClick={() => setFilterType("all")}
          style={{ padding: "4px 10px", fontSize: "12px", height: "auto" }}
        >
          全部 ({graph.nodes.length})
        </button>
        <button
          className={`button small ${filterType === "heavy" ? "primary" : "ghost"}`}
          onClick={() => setFilterType("heavy")}
          style={{ padding: "4px 10px", fontSize: "12px", height: "auto" }}
        >
          重依赖 ({graph.summary.heavyNodes})
        </button>
        <button
          className={`button small ${filterType === "light" ? "primary" : "ghost"}`}
          onClick={() => setFilterType("light")}
          style={{ padding: "4px 10px", fontSize: "12px", height: "auto" }}
        >
          轻依赖 ({graph.summary.lightNodes})
        </button>
        <button
          className={`button small ${filterType === "shared" ? "primary" : "ghost"}`}
          onClick={() => setFilterType("shared")}
          style={{ padding: "4px 10px", fontSize: "12px", height: "auto" }}
        >
          共享依赖 ({graph.summary.sharedNodes})
        </button>
      </div>

      <div className="result-table" role="table" aria-label="依赖包清单">
        <div className="result-row result-head" role="row">
          <span role="columnheader">依赖包名</span>
          <span role="columnheader">依赖关系</span>
          <span role="columnheader">引入自 (根包)</span>
          <span role="columnheader">最新版本</span>
          <span role="columnheader">子依赖数量</span>
        </div>
        {filteredNodes.length === 0 ? (
          <div style={{ textAlign: "center", padding: "20px", color: "#999" }}>没有匹配的依赖包</div>
        ) : (
          filteredNodes.map((node, index) => {
            const isShared = node.rootPackages.length > 1;
            const isRoot = graph.roots.includes(node.package);
            return (
              <div className="result-row" role="row" key={`${node.package}-${index}`}>
                <strong role="cell">
                  {node.package}
                  {isShared && (
                    <span
                      className="source-tag"
                      style={{
                        marginLeft: "8px",
                        backgroundColor: "#ebd6ff",
                        color: "#6600cc",
                        fontSize: "10px",
                        padding: "2px 4px",
                      }}
                    >
                      共享依赖
                    </span>
                  )}
                </strong>
                <span role="cell">
                  {isRoot ? "根包" : `深度 ${node.depth}`}
                </span>
                <span role="cell" style={{ fontSize: "12px" }}>{node.rootPackages.join(", ")}</span>
                <code role="cell">{node.version}</code>
                <span role="cell">
                  {node.directDependencyCount} 个 (重: {node.heavyDependencyCount})
                </span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

function getInstallCommand(result: SearchResult): string {
  if (!result.found) return result.package;
  if (result.source === "cran") {
    return `install.packages("${result.package}")`;
  }
  if (result.source === "bioc") {
    return `BiocManager::install("${result.package}", update = FALSE, ask = FALSE)`;
  }
  if (result.source === "biocGit") {
    const version = result.latestVersion || "";
    const biocVer = version.split("|")[1] || "3.18";
    const release = `RELEASE_${biocVer.replace(".", "_")}`;
    return `remotes::install_git("https://git.bioconductor.org/packages/${result.package}", ref = "${release}", upgrade = "never")`;
  }
  if (result.source === "github" && result.repository) {
    if (result.latestVersion) {
      const cleanVer = result.latestVersion.startsWith("v") ? result.latestVersion : `v${result.latestVersion}`;
      return `remotes::install_github("${result.repository}@${cleanVer}", upgrade = "never")`;
    }
    return `remotes::install_github("${result.repository}", upgrade = "never")`;
  }
  return `install.packages("${result.package}")`;
}

export function ReportView({
  results,
  logs,
  dependencyGraph,
  packageCount,
  uniqueFoundCount,
  smartSuggestions,
  searching,
  searchDuration,
  onClearLogs,
  onStatusChange,
  onApplySmartSuggestion,
  onRetryMissing,
}: ReportViewProps) {
  const [activeTab, setActiveTab] = useState<"graph" | "list">("graph");
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [resultFilter, setResultFilter] = useState<"all" | "found" | "missing" | "error">("all");
  const [resultSearch, setResultSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [sortKey, setSortKey] = useState<"package" | "source" | "version" | "status">("package");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("asc");
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; result: SearchResult } | null>(null);
  const [selectedRowIndex, setSelectedRowIndex] = useState<number>(-1);
  const resultTableRef = useRef<HTMLDivElement>(null);
  const [logSearch, setLogSearch] = useState("");
  const [expandedRow, setExpandedRow] = useState<string | null>(null);
  const [selectedResults, setSelectedResults] = useState<Set<string>>(new Set());
  const [compactMode, setCompactMode] = useState(false);
  const [logWrap, setLogWrap] = useState(false);
  const [sourceFilter, setSourceFilter] = useState<string | null>(null);

  useEffect(() => {
    if (!ctxMenu) return;
    function closeCtxMenu() { setCtxMenu(null); }
    window.addEventListener("click", closeCtxMenu);
    window.addEventListener("scroll", closeCtxMenu, true);
    return () => {
      window.removeEventListener("click", closeCtxMenu);
      window.removeEventListener("scroll", closeCtxMenu, true);
    };
  }, [ctxMenu]);

  useEffect(() => {
    const timer = window.setTimeout(() => setDebouncedSearch(resultSearch), 200);
    return () => window.clearTimeout(timer);
  }, [resultSearch]);

  const logConsoleRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (logConsoleRef.current) {
      logConsoleRef.current.scrollTop = logConsoleRef.current.scrollHeight;
    }
  }, [logs]);

  useEffect(() => {
    function onKeydown(e: KeyboardEvent) {
      if (e.key === "Escape" && (resultFilter !== "all" || resultSearch)) {
        setResultFilter("all");
        setResultSearch("");
        setDebouncedSearch("");
      } else if (e.altKey && e.key === "1") {
        e.preventDefault();
        setActiveTab("graph");
      } else if (e.altKey && e.key === "2") {
        e.preventDefault();
        setActiveTab("list");
      }
    }
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  }, [resultFilter, resultSearch]);

  const filteredResults = useMemo(() => {
    let list = results;
    if (resultFilter === "found") list = list.filter((r) => r.found);
    else if (resultFilter === "missing") list = list.filter((r) => !r.found && r.status !== "timeout" && r.status !== "rateLimited" && r.status !== "error");
    else if (resultFilter === "error") list = list.filter((r) => !r.found && (r.status === "timeout" || r.status === "rateLimited" || r.status === "error"));
    if (sourceFilter) list = list.filter((r) => r.source === sourceFilter);
    const q = debouncedSearch.trim().toLowerCase();
    if (q) list = list.filter((r) => r.package.toLowerCase().includes(q) || (r.repository && r.repository.toLowerCase().includes(q)));
    return list;
  }, [results, resultFilter, debouncedSearch, sourceFilter]);

  const missingCount = useMemo(
    () => new Set(results.filter((r) => !r.found && r.status !== "timeout" && r.status !== "rateLimited" && r.status !== "error").map((r) => r.package)).size,
    [results],
  );
  const errorCount = useMemo(
    () => new Set(results.filter((r) => !r.found && (r.status === "timeout" || r.status === "rateLimited" || r.status === "error")).map((r) => r.package)).size,
    [results],
  );

  const toggleFilter = useCallback((filter: "found" | "missing" | "error") => {
    setResultFilter((prev) => (prev === filter ? "all" : filter));
  }, []);

  const toggleSort = useCallback((key: "package" | "source" | "version" | "status") => {
    setSortKey((prevKey) => {
      if (prevKey === key) {
        setSortDir((prevDir) => (prevDir === "asc" ? "desc" : "asc"));
        return prevKey;
      }
      setSortDir("asc");
      return key;
    });
  }, []);

  const statusRank = (r: SearchResult): number => {
    if (r.found) return 0;
    if (r.status === "timeout") return 1;
    if (r.status === "rateLimited") return 2;
    if (r.status === "error") return 3;
    return 4;
  };

  const sortedResults = useMemo(() => {
    const dir = sortDir === "asc" ? 1 : -1;
    return [...filteredResults].sort((a, b) => {
      let cmp = 0;
      if (sortKey === "package") cmp = a.package.localeCompare(b.package);
      else if (sortKey === "source") cmp = a.source.localeCompare(b.source);
      else if (sortKey === "version") cmp = (a.latestVersion || "").localeCompare(b.latestVersion || "", undefined, { numeric: true });
      else if (sortKey === "status") cmp = statusRank(a) - statusRank(b);
      return cmp * dir;
    });
  }, [filteredResults, sortKey, sortDir]);

  useEffect(() => {
    setSelectedRowIndex(-1);
  }, [filteredResults]);

  useEffect(() => {
    setSelectedResults(new Set());
  }, [filteredResults]);

  useEffect(() => {
    function onKeydown(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if (sortedResults.length === 0) return;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedRowIndex((prev) => Math.min(prev + 1, sortedResults.length - 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedRowIndex((prev) => Math.max(prev - 1, 0));
      } else if (e.key === "Enter" && selectedRowIndex >= 0) {
        e.preventDefault();
        const row = sortedResults[selectedRowIndex];
        if (row) handleCopy(row, `${row.package}-kbd`);
      }
    }
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  }, [sortedResults, selectedRowIndex]);

  const handleCopy = async (result: SearchResult, key: string) => {
    const cmd = getInstallCommand(result);
    try {
      await navigator.clipboard.writeText(cmd);
      setCopiedKey(key);
      onStatusChange(result.found ? `已复制 ${result.package} 的安装指令` : `已复制包名 ${result.package}`);
      setTimeout(() => setCopiedKey(null), 1500);
    } catch (error) {
      onStatusChange(`复制安装指令失败: ${error instanceof Error ? error.message : String(error)}`);
    }
  };

  const handleOpenPage = async (result: SearchResult) => {
    if (!result.found) return;
    if (result.source !== "cran" && result.source !== "bioc" && result.source !== "github") return;
    try {
      await invoke("open_package_page", {
        package: result.realName || result.package,
        source: result.source,
        repository: result.repository || "",
      });
      onStatusChange(`已打开 ${result.package} 的来源网页`);
    } catch (error) {
      onStatusChange(`打开来源网页失败: ${error instanceof Error ? error.message : String(error)}`);
    }
  };

  return (
    <div className={`report-layout ${dependencyGraph ? "has-deps" : ""}`}>
      <div className="metric-row">
        <Metric label="输入包" value={packageCount} />
        <Metric
          label="已验证包"
          value={uniqueFoundCount}
          tone="success"
          active={resultFilter === "found"}
          onClick={() => toggleFilter("found")}
        />
        <Metric
          label="未找到"
          value={missingCount}
          tone="danger"
          active={resultFilter === "missing"}
          onClick={() => toggleFilter("missing")}
        />
        <Metric
          label="异常"
          value={errorCount}
          tone="warning"
          active={resultFilter === "error"}
          onClick={() => toggleFilter("error")}
        />
        <Metric label="来源记录" value={results.length} />
        {(() => {
          const total = uniqueFoundCount + missingCount + errorCount;
          if (total === 0) return null;
          const foundPct = uniqueFoundCount / total;
          const missingPct = missingCount / total;
          const errorPct = errorCount / total;
          const r = 28;
          const C = 2 * Math.PI * r;
          const seg = (pct: number, offset: number) => {
            const len = C * pct;
            const gap = C - len;
            return `${len} ${gap} ${offset}`;
          };
          return (
            <div className="mini-pie-wrapper" title={`已验证 ${uniqueFoundCount} · 未找到 ${missingCount} · 异常 ${errorCount}`}>
              <svg width="72" height="72" viewBox="0 0 72 72" style={{ transform: "rotate(-90deg)" }}>
                <circle cx="36" cy="36" r={r} fill="none" stroke="var(--tag-default-bg)" strokeWidth="8" />
                <circle cx="36" cy="36" r={r} fill="none" stroke="var(--success-color)" strokeWidth="8"
                  strokeDasharray={seg(foundPct, 0)} strokeLinecap="butt" />
                <circle cx="36" cy="36" r={r} fill="none" stroke="var(--red)" strokeWidth="8"
                  strokeDasharray={seg(missingPct, C * foundPct)} strokeLinecap="butt" />
                <circle cx="36" cy="36" r={r} fill="none" stroke="#e67e22" strokeWidth="8"
                  strokeDasharray={seg(errorPct, C * (foundPct + missingPct))} strokeLinecap="butt" />
              </svg>
              <span className="mini-pie-center">{Math.round(foundPct * 100)}%</span>
            </div>
          );
        })()}
        {(() => {
          const sourceCounts = results.reduce<Record<string, number>>((acc, r) => {
            if (r.found) { acc[r.source] = (acc[r.source] || 0) + 1; }
            return acc;
          }, {});
          const entries = Object.entries(sourceCounts).sort((a, b) => b[1] - a[1]).slice(0, 5);
          if (entries.length === 0) return null;
          const max = Math.max(...entries.map(([, c]) => c));
          const colors: Record<string, string> = { cran: "#176b4d", bioc: "#ca8a04", biocGit: "#ca8a04", github: "#4865aa", unknown: "#64748b" };
          return (
            <div className="source-bar-chart" title="已验证包来源分布">
              {entries.map(([src, count]) => (
                <div key={src} className="source-bar-item" title={`${sourceNames[src] ?? src}: ${count}`}>
                  <span className="source-bar-label">{sourceNames[src] ?? src}</span>
                  <div className="source-bar-track">
                    <div className="source-bar-fill" style={{ width: `${(count / max) * 100}%`, background: colors[src] || "#64748b" }} />
                  </div>
                  <span className="source-bar-count">{count}</span>
                </div>
              ))}
            </div>
          );
        })()}
      </div>

      <section className="panel report-panel">
        {searching && <div className="search-progress-bar" />}
        <div className="report-panel-header">
          <PanelHeader
            step="结果"
            title="来源验证"
            meta={
              searching
                ? "实时更新"
                : searchDuration != null
                ? `已完成 · ${(searchDuration / 1000).toFixed(1)}s`
                : "已完成"
            }
          />
          {results.length > 0 && (
            <div style={{ display: "flex", gap: "6px" }}>
              {selectedResults.size > 0 && (
                <button
                  type="button"
                  className="button ghost compact-btn"
                  onClick={async () => {
                    const cmds = sortedResults
                      .filter((r) => selectedResults.has(r.package))
                      .map((r) => getInstallCommand(r));
                    const unique = [...new Set(cmds)];
                    try {
                      await navigator.clipboard.writeText(unique.join("\n"));
                      onStatusChange(`已复制 ${unique.length} 条选中命令`);
                    } catch (err) {
                      onStatusChange(`复制失败: ${err instanceof Error ? err.message : String(err)}`);
                    }
                  }}
                >
                  复制选中({selectedResults.size})
                </button>
              )}
              {results.some((r) => r.found) && (
                <>
                  <button
                    type="button"
                    className="button ghost compact-btn"
                    onClick={async () => {
                      const cmds = results.filter((r) => r.found).map((r) => getInstallCommand(r));
                      const unique = [...new Set(cmds)];
                      try {
                        await navigator.clipboard.writeText(unique.join("\n"));
                        onStatusChange(`已复制 ${unique.length} 条安装指令`);
                      } catch (err) {
                        onStatusChange(`复制失败: ${err instanceof Error ? err.message : String(err)}`);
                      }
                    }}
                  >
                    复制全部指令
                  </button>
                  <button
                    type="button"
                    className="button ghost compact-btn"
                    onClick={async () => {
                      const names = [...new Set(results.filter((r) => r.found).map((r) => r.package))];
                      try {
                        await navigator.clipboard.writeText(names.join("\n"));
                        onStatusChange(`已复制 ${names.length} 个包名`);
                      } catch (err) {
                        onStatusChange(`复制失败: ${err instanceof Error ? err.message : String(err)}`);
                      }
                    }}
                  >
                    复制包名
                  </button>
                  <button
                    type="button"
                    className="button ghost compact-btn"
                    onClick={async () => {
                      const found = results.filter((r) => r.found);
                      const cmds = [...new Set(found.map((r) => getInstallCommand(r)))];
                      const cran = cmds.filter((c) => c.includes("install.packages"));
                      const bioc = cmds.filter((c) => c.includes("BiocManager"));
                      const github = cmds.filter((c) => c.includes("remotes") || c.includes("devtools"));
                      const lines: string[] = [
                        "# ============================================================",
                        "# R Package Installation Script",
                        `# Generated: ${new Date().toLocaleString("zh-CN")}`,
                        `# Packages: ${cmds.length}`,
                        "# ============================================================",
                        "",
                      ];
                      if (bioc.length > 0) {
                        lines.push('if (!requireNamespace("BiocManager", quietly = TRUE))', '    install.packages("BiocManager")', "");
                      }
                      if (github.length > 0) {
                        lines.push('if (!requireNamespace("remotes", quietly = TRUE))', '    install.packages("remotes")', "");
                      }
                      if (cran.length > 0) {
                        lines.push("# --- CRAN ---", ...cran, "");
                      }
                      if (bioc.length > 0) {
                        lines.push("# --- Bioconductor ---", ...bioc, "");
                      }
                      if (github.length > 0) {
                        lines.push("# --- GitHub ---", ...github, "");
                      }
                      lines.push('# cat("\\n Installation complete.\\n")');
                      try {
                        await navigator.clipboard.writeText(lines.join("\n"));
                        onStatusChange(`已复制完整安装脚本（${cmds.length} 个包）`);
                      } catch (err) {
                        onStatusChange(`复制失败: ${err instanceof Error ? err.message : String(err)}`);
                      }
                    }}
                  >
                    复制为脚本
                  </button>
                  <button
                    type="button"
                    className="button ghost compact-btn"
                    onClick={async () => {
                      const found = results.filter(
                        (r) => r.found && (r.source === "cran" || r.source === "bioc" || r.source === "github"),
                      );
                      const unique = new Map<string, SearchResult>();
                      for (const r of found) {
                        const key = `${r.source}:${r.package}`;
                        if (!unique.has(key)) unique.set(key, r);
                      }
                      if (unique.size > 5 && !window.confirm(`将要打开 ${unique.size} 个浏览器页面，是否继续？`)) {
                        return;
                      }
                      let opened = 0;
                      let failed = 0;
                      for (const r of unique.values()) {
                        try {
                          await invoke("open_package_page", {
                            package: r.realName || r.package,
                            source: r.source,
                            repository: r.repository || "",
                          });
                          opened += 1;
                          await new Promise((res) => window.setTimeout(res, 150));
                        } catch {
                          failed += 1;
                        }
                      }
                      onStatusChange(`已打开 ${opened} 个来源网页${failed > 0 ? `，失败 ${failed} 个` : ""}`);
                    }}
                  >
                    打开来源网页
                  </button>
                </>
              )}
              {results.some((r) => !r.found) && (
                <>
                  <button
                    type="button"
                    className="button ghost compact-btn"
                    onClick={async () => {
                      const missing = [...new Set(
                        results.filter((r) => !r.found).map((r) => r.package),
                      )];
                      try {
                        await navigator.clipboard.writeText(missing.join("\n"));
                        onStatusChange(`已复制 ${missing.length} 个未找到的包名`);
                      } catch (err) {
                        onStatusChange(`复制失败: ${err instanceof Error ? err.message : String(err)}`);
                      }
                    }}
                  >
                    复制未找到
                  </button>
                  <button
                    type="button"
                    className="button ghost compact-btn"
                    disabled={searching}
                    onClick={() => {
                      const missing = [...new Set(
                        results.filter((r) => !r.found).map((r) => r.package),
                      )];
                      onRetryMissing(missing);
                    }}
                  >
                    重试未找到
                  </button>
                  {results.some((r) => !r.found && (r.status === "timeout" || r.status === "rateLimited" || r.status === "error")) && (
                    <button
                      type="button"
                      className="button ghost compact-btn"
                      disabled={searching}
                      onClick={() => {
                        const errorPkgs = [...new Set(
                          results
                            .filter((r) => !r.found && (r.status === "timeout" || r.status === "rateLimited" || r.status === "error"))
                            .map((r) => r.package),
                        )];
                        onRetryMissing(errorPkgs);
                      }}
                    >
                      重试异常
                    </button>
                  )}
                </>
              )}
              <button
                type="button"
                className="button ghost compact-btn"
                onClick={() => {
                  const escape = (s: string) => {
                    const v = s ?? "";
                    return v.includes(",") || v.includes('"') || v.includes("\n")
                      ? `"${v.replace(/"/g, '""')}"`
                      : v;
                  };
                  const header = ["包名", "来源", "版本", "仓库", "状态", "安装命令"].join(",");
                  const rows = results.map((r) =>
                    [
                      escape(r.package),
                      escape(sourceNames[r.source] ?? r.source),
                      escape(r.latestVersion),
                      escape(r.repository),
                      escape(
                        r.found
                          ? "已验证"
                          : r.status === "timeout"
                          ? "超时"
                          : r.status === "rateLimited"
                          ? "频率限制"
                          : r.status === "error"
                          ? "检索异常"
                          : "未找到",
                      ),
                      escape(r.found ? getInstallCommand(r) : ""),
                    ].join(","),
                  );
                  const csv = "\uFEFF" + [header, ...rows].join("\r\n");
                  const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
                  const url = URL.createObjectURL(blob);
                  const a = document.createElement("a");
                  a.href = url;
                  a.download = "r_package_results.csv";
                  document.body.appendChild(a);
                  a.click();
                  document.body.removeChild(a);
                  URL.revokeObjectURL(url);
                  onStatusChange(`已导出 ${results.length} 条结果至 CSV`);
                }}
              >
                导出 CSV
              </button>
              <button
                type="button"
                className="button ghost compact-btn"
                onClick={async () => {
                  const json = JSON.stringify({
                    generatedAt: new Date().toISOString(),
                    searchDurationMs: searchDuration,
                    packageCount,
                    uniqueFoundCount,
                    results: results.map((r) => ({
                      package: r.package,
                      source: r.source,
                      version: r.latestVersion || null,
                      repository: r.repository || null,
                      found: r.found,
                      status: r.found ? "found" : r.status,
                      installCommand: getInstallCommand(r),
                    })),
                  }, null, 2);
                  try {
                    await navigator.clipboard.writeText(json);
                    onStatusChange(`已复制 JSON（${results.length} 条结果）`);
                  } catch (err) {
                    onStatusChange(`复制失败: ${err instanceof Error ? err.message : String(err)}`);
                  }
                }}
              >
                复制 JSON
              </button>
              <button
                type="button"
                className="button ghost compact-btn"
                onClick={async () => {
                  const found = results.filter((r) => r.found);
                  const missing = results.filter((r) => !r.found && r.status !== "timeout" && r.status !== "rateLimited" && r.status !== "error");
                  const errors = results.filter((r) => !r.found && (r.status === "timeout" || r.status === "rateLimited" || r.status === "error"));
                  const foundPkgs = [...new Set(found.map((r) => r.package))];
                  const missingPkgs = [...new Set(missing.map((r) => r.package))];
                  const errorPkgs = [...new Set(errors.map((r) => r.package))];
                  const now = new Date();
                  const lines = [
                    `R Package Center 检索报告`,
                    `时间: ${now.toLocaleString("zh-CN")}`,
                    searchDuration != null ? `耗时: ${(searchDuration / 1000).toFixed(1)}s` : "",
                    ``,
                    `输入包: ${packageCount}`,
                    `已验证: ${foundPkgs.length}`,
                    `未找到: ${missingPkgs.length}`,
                    `异常: ${errorPkgs.length}`,
                    `来源记录: ${results.length}`,
                  ].filter(Boolean);
                  if (foundPkgs.length > 0) {
                    lines.push("", "已验证包:", ...foundPkgs.map((p) => `  - ${p}`));
                  }
                  if (missingPkgs.length > 0) {
                    lines.push("", "未找到包:", ...missingPkgs.map((p) => `  - ${p}`));
                  }
                  if (errorPkgs.length > 0) {
                    lines.push("", "异常包:", ...errorPkgs.map((p) => `  - ${p}`));
                  }
                  try {
                    await navigator.clipboard.writeText(lines.join("\n"));
                    onStatusChange("已复制检索结果摘要");
                  } catch (err) {
                    onStatusChange(`复制失败: ${err instanceof Error ? err.message : String(err)}`);
                  }
                }}
              >
                复制摘要
              </button>
            </div>
          )}
        </div>
        {smartSuggestions.length > 0 && (
          <div className="smart-suggestion-list report-suggestions" aria-label="检索智能建议">
            {smartSuggestions.map((suggestion) => (
              <div className="smart-suggestion" key={suggestion.id}>
                <div>
                  <strong>{suggestion.title}</strong>
                  <span>{suggestion.detail}</span>
                </div>
                {suggestion.actionLabel && (
                  <button type="button" className="text-button" onClick={() => onApplySmartSuggestion(suggestion)}>
                    {suggestion.actionLabel}
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
        {results.length === 0 ? (
          <EmptyState
            text={searching ? "正在等待首条检索结果" : "尚未执行检索"}
            hint={searching ? undefined : "请在「工作台」页面输入包名并点击「开始检索」"}
          />
        ) : (
          <>
            <div className="result-filter-bar">
              <div className="result-filter-tabs">
                {([
                  { key: "all", label: `全部 ${results.length}` },
                  { key: "found", label: `已验证 ${results.filter((r) => r.found).length}` },
                  { key: "missing", label: `未找到 ${results.filter((r) => !r.found && r.status !== "timeout" && r.status !== "rateLimited" && r.status !== "error").length}` },
                  { key: "error", label: `异常 ${results.filter((r) => !r.found && (r.status === "timeout" || r.status === "rateLimited" || r.status === "error")).length}` },
                ] as const).map((tab) => (
                  <button
                    key={tab.key}
                    type="button"
                    className={`result-filter-tab ${resultFilter === tab.key ? "active" : ""}`}
                    onClick={() => setResultFilter(tab.key)}
                  >
                    {tab.label}
                  </button>
                ))}
              </div>
              <input
                type="text"
                className="result-search-input"
                placeholder="筛选包名或仓库..."
                value={resultSearch}
                onChange={(e) => setResultSearch(e.target.value)}
              />
              {(debouncedSearch || resultFilter !== "all" || sourceFilter) && (
                <small className="result-count-hint">
                  显示 {filteredResults.length}/{results.length} 条
                  {sourceFilter && (
                    <button
                      type="button"
                      className="source-filter-clear"
                      onClick={() => setSourceFilter(null)}
                      title="取消来源筛选"
                    >
                      {sourceNames[sourceFilter] ?? sourceFilter} ✕
                    </button>
                  )}
                </small>
              )}
              <button
                type="button"
                className={`button ghost compact-btn${compactMode ? " active" : ""}`}
                onClick={() => setCompactMode((v) => !v)}
                title={compactMode ? "当前：紧凑模式，点击切换为标准" : "当前：标准模式，点击切换为紧凑"}
                style={{ marginLeft: "auto" }}
              >
                {compactMode ? "紧凑✓" : "紧凑"}
              </button>
            </div>
            {filteredResults.length === 0 ? (
              <EmptyState text="当前筛选条件下无匹配结果" hint="尝试切换上方的筛选标签或清空搜索框" />
            ) : (
              <div className="result-table-wrapper" ref={resultTableRef}>
                <div className={`result-table${compactMode ? " compact" : ""}`} role="table" aria-label="包来源验证结果">
                  <div className="result-row result-head" role="row">
                    <span role="columnheader" className="result-check-cell">
                      <input
                        type="checkbox"
                        checked={sortedResults.length > 0 && sortedResults.every((r) => selectedResults.has(r.package))}
                        onChange={() => {
                          const allSelected = sortedResults.length > 0 && sortedResults.every((r) => selectedResults.has(r.package));
                          if (allSelected) setSelectedResults(new Set());
                          else setSelectedResults(new Set(sortedResults.map((r) => r.package)));
                        }}
                        aria-label="全选"
                      />
                    </span>
                    <span role="columnheader" className={`sortable ${sortKey === "package" ? `sorted-${sortDir}` : ""}`} onClick={() => toggleSort("package")}>包名</span>
                    <span role="columnheader" className={`sortable ${sortKey === "source" ? `sorted-${sortDir}` : ""}`} onClick={() => toggleSort("source")}>来源</span>
                    <span role="columnheader" className={`sortable ${sortKey === "version" ? `sorted-${sortDir}` : ""}`} onClick={() => toggleSort("version")}>版本</span>
                    <span role="columnheader">仓库</span>
                    <span role="columnheader" className={`sortable ${sortKey === "status" ? `sorted-${sortDir}` : ""}`} onClick={() => toggleSort("status")}>状态</span>
                  </div>
                  {sortedResults.map((result, index) => {
                const rowKey = `${result.package}-${result.source}-${index}`;
                const isCopied = copiedKey === rowKey;
                const installCmd = getInstallCommand(result);
                const isExpanded = expandedRow === rowKey;
                return (
                  <Fragment key={rowKey}>
                  <div
                    className={`result-row${selectedRowIndex === index ? " row-selected" : ""}`}
                    role="row"
                    onDoubleClick={() => handleCopy(result, rowKey)}
                    onMouseEnter={() => setSelectedRowIndex(index)}
                    onContextMenu={(e) => { e.preventDefault(); setCtxMenu({ x: e.clientX, y: e.clientY, result }); }}
                    title={result.found ? "双击此行可复制安装命令" : "双击此行可复制包名"}
                  >
                    <span role="cell" className="result-check-cell">
                      <input
                        type="checkbox"
                        checked={selectedResults.has(result.package)}
                        onChange={() => {
                          setSelectedResults((prev) => {
                            const next = new Set(prev);
                            if (next.has(result.package)) next.delete(result.package);
                            else next.add(result.package);
                            return next;
                          });
                        }}
                        aria-label={`选择 ${result.package}`}
                      />
                    </span>
                    <strong
                      role="cell"
                      className={result.found && (result.source === "cran" || result.source === "bioc" || result.source === "github") ? "pkg-link" : ""}
                      onClick={() => handleOpenPage(result)}
                      title={result.found && (result.source === "cran" || result.source === "bioc" || result.source === "github") ? `点击打开 ${result.package} 来源网页` : undefined}
                    >{result.package}</strong>
                    <span role="cell" className="source-cell-with-copy">
                      <span
                        className={`source-tag ${result.source}${sourceFilter === result.source ? " tag-active" : ""}`}
                        style={{ cursor: "pointer" }}
                        onClick={() => setSourceFilter(sourceFilter === result.source ? null : result.source)}
                        title={sourceFilter === result.source ? `点击取消 ${sourceNames[result.source] ?? result.source} 筛选` : `点击筛选 ${sourceNames[result.source] ?? result.source} 来源`}
                      >
                        {sourceNames[result.source] ?? result.source}
                      </span>
                      <button
                        type="button"
                        className={`row-copy-btn ${isCopied ? "copied" : ""}`}
                        title={result.found ? `复制安装指令: ${installCmd}` : `复制包名: ${result.package}`}
                        onClick={() => handleCopy(result, rowKey)}
                      >
                        {isCopied ? (
                          <svg className="copy-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                            <polyline points="20 6 9 17 4 12" />
                          </svg>
                        ) : (
                          <svg className="copy-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                            <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                          </svg>
                        )}
                      </button>
                    </span>
                    <code role="cell">{result.latestVersion || "—"}</code>
                    <span role="cell" className="repo-cell">{result.repository || "—"}</span>
                    <span
                      role="cell"
                      className={
                        result.found
                          ? "found"
                          : result.status === "timeout"
                          ? "timeout"
                          : result.status === "rateLimited"
                          ? "rate-limited"
                          : result.status === "error"
                          ? "error"
                          : "missing"
                      }
                    >
                      {result.status === "timeout"
                        ? "超时"
                        : result.status === "rateLimited"
                        ? "频率限制"
                        : result.status === "error"
                        ? "检索异常"
                        : result.found
                        ? "已验证"
                        : "未找到"}
                    </span>
                    <button
                      type="button"
                      className="row-expand-btn"
                      title={isExpanded ? "收起详情" : "展开详情"}
                      onClick={() => setExpandedRow(isExpanded ? null : rowKey)}
                    >
                      {isExpanded ? "▴" : "▾"}
                    </button>
                  </div>
                  {isExpanded && (
                    <div className="result-detail">
                      <div className="result-detail-grid">
                        <div><span className="detail-label">安装命令</span><code className="detail-code">{installCmd}</code></div>
                        {result.realName && result.realName !== result.package && (
                          <div><span className="detail-label">规范名称</span><span>{result.realName}</span></div>
                        )}
                        <div><span className="detail-label">仓库地址</span><span>{result.repository || "—"}</span></div>
                        <div><span className="detail-label">检索状态</span><span>{result.found ? "已验证" : result.status === "timeout" ? "超时" : result.status === "rateLimited" ? "频率限制" : result.status === "error" ? "检索异常" : "未找到"}</span></div>
                      </div>
                    </div>
                  )}
                  </Fragment>
                );
              })}
                  </div>
                </div>
              )}
            </>
          )}
      </section>

      {ctxMenu && (
        <div
          className="ctx-menu"
          style={{ position: "fixed", left: ctxMenu.x, top: ctxMenu.y, zIndex: 1000 }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            type="button"
            className="ctx-menu-item"
            onClick={() => { handleCopy(ctxMenu.result, `${ctxMenu.result.package}-ctx`); setCtxMenu(null); }}
          >
            {ctxMenu.result.found ? "复制安装命令" : "复制包名"}
          </button>
          {ctxMenu.result.found && (ctxMenu.result.source === "cran" || ctxMenu.result.source === "bioc" || ctxMenu.result.source === "github") && (
            <button
              type="button"
              className="ctx-menu-item"
              onClick={() => { handleOpenPage(ctxMenu.result); setCtxMenu(null); }}
            >
              打开来源网页
            </button>
          )}
          {!ctxMenu.result.found && (
            <button
              type="button"
              className="ctx-menu-item"
              onClick={() => { onRetryMissing([ctxMenu.result.package]); setCtxMenu(null); }}
            >
              重试此包
            </button>
          )}
        </div>
      )}

      {dependencyGraph && (
        <section className="panel dependency-panel">
          <PanelHeader
            step="扩展"
            title="依赖关系智能分析"
            meta={`共构建 ${dependencyGraph.summary.totalNodes} 个包节点，${dependencyGraph.summary.totalEdges} 条依赖链路`}
          />

          <div className="dependency-content-wrapper">
            <div
              className="tab-header"
              style={{
                display: "flex",
                gap: "16px",
                marginBottom: "16px",
                borderBottom: "1px solid var(--border-color)",
                paddingBottom: "8px",
              }}
            >
              <button
                className={`tab-btn ${activeTab === "graph" ? "active" : ""}`}
                onClick={() => setActiveTab("graph")}
                style={{
                  background: "none",
                  border: "none",
                  fontSize: "14px",
                  padding: "6px 12px",
                  cursor: "pointer",
                  borderBottom: activeTab === "graph" ? "2px solid var(--primary-color)" : "none",
                  fontWeight: activeTab === "graph" ? "600" : "normal",
                  color: activeTab === "graph" ? "var(--primary-color)" : "inherit",
                }}
              >
                依赖关系图谱<span className="kbd-hint">Alt+1</span>
              </button>
              <button
                className={`tab-btn ${activeTab === "list" ? "active" : ""}`}
                onClick={() => setActiveTab("list")}
                style={{
                  background: "none",
                  border: "none",
                  fontSize: "14px",
                  padding: "6px 12px",
                  cursor: "pointer",
                  borderBottom: activeTab === "list" ? "2px solid var(--primary-color)" : "none",
                  fontWeight: activeTab === "list" ? "600" : "normal",
                  color: activeTab === "list" ? "var(--primary-color)" : "inherit",
                }}
              >
                依赖清单列表<span className="kbd-hint">Alt+2</span>
              </button>
            </div>

            {activeTab === "graph" ? (
              <DependencyGraphView graph={dependencyGraph} />
            ) : (
              <DependencyListView graph={dependencyGraph} />
            )}
          </div>
        </section>
      )}

      <section className="panel log-panel">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <PanelHeader step="日志" title="检索过程" meta={`${logs.length} 行`} />
          <div style={{ display: "flex", gap: "8px", alignItems: "center", marginRight: "16px" }}>
            <label className="log-wrap-toggle" title="切换日志自动换行">
              <input
                type="checkbox"
                checked={logWrap}
                onChange={(e) => setLogWrap(e.target.checked)}
              />
              <span>换行</span>
            </label>
            <input
              type="text"
              placeholder="过滤日志..."
              value={logSearch}
              onChange={(e) => setLogSearch(e.target.value)}
              style={{ padding: "4px 8px", fontSize: "12px", width: "120px", borderRadius: "4px", border: "1px solid var(--line)", background: "var(--input-bg, #fff)", color: "var(--ink)" }}
            />
            <button
              className="button ghost"
              style={{
                padding: "4px 8px",
                fontSize: "12px",
                height: "auto",
              }}
              onClick={onClearLogs}
              disabled={searching || logs.length === 0}
            >
              清除日志
            </button>
          </div>
        </div>
        <div className={`log-console${logWrap ? " log-wrap" : ""}`} ref={logConsoleRef}>
          {(() => {
            const q = logSearch.trim().toLowerCase();
            const filtered = q ? logs.filter((line) => line.toLowerCase().includes(q)) : logs;
            return filtered.length ? (
              filtered.map((line, index) => (
                <div key={`${line}-${index}`}>
                  <span>{String(index + 1).padStart(2, "0")}</span>
                  {line}
                </div>
              ))
            ) : (
              <EmptyState text={q ? "无匹配日志" : "日志将在检索开始后显示"} />
            );
          })()}
        </div>
      </section>
    </div>
  );
}
