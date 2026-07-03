import { useState, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PanelHeader, Metric, EmptyState } from "./components";
import { sourceNames } from "./types";
import type { SearchResult, DependencyGraph, DependencyNode, ReverseDependenciesInfo } from "./utils";

interface ReportViewProps {
  results: SearchResult[];
  logs: string[];
  dependencyGraph: DependencyGraph | null;
  packageCount: number;
  uniqueFoundCount: number;
  searching: boolean;
  onClearLogs: () => void;
  onStatusChange: (status: string) => void;
}

function DependencyGraphView({ graph }: { graph: DependencyGraph }) {
  const [hoveredNode, setHoveredNode] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<DependencyNode | null>(null);
  const [filterStrength, setFilterStrength] = useState<"all" | "heavy">("all");
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
                  opacity={opacity}
                  style={{ overflow: "visible", cursor: "pointer", transition: "opacity 0.2s" }}
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
  searching,
  onClearLogs,
  onStatusChange,
}: ReportViewProps) {
  const [activeTab, setActiveTab] = useState<"graph" | "list">("graph");
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

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

  return (
    <div className={`report-layout ${dependencyGraph ? "has-deps" : ""}`}>
      <div className="metric-row">
        <Metric label="输入包" value={packageCount} />
        <Metric label="已验证包" value={uniqueFoundCount} tone="success" />
        <Metric
          label="未找到"
          value={new Set(results.filter((item) => !item.found).map((item) => item.package)).size}
          tone="danger"
        />
        <Metric label="来源记录" value={results.length} />
      </div>

      <section className="panel report-panel">
        <PanelHeader step="结果" title="来源验证" meta={searching ? "实时更新" : "已完成"} />
        {results.length === 0 ? (
          <EmptyState text={searching ? "正在等待首条检索结果" : "尚未执行检索"} />
        ) : (
          <div className="result-table-wrapper">
            <div className="result-table" role="table" aria-label="包来源验证结果">
              <div className="result-row result-head" role="row">
                <span role="columnheader">包名</span>
                <span role="columnheader">来源</span>
                <span role="columnheader">版本</span>
                <span role="columnheader">仓库</span>
                <span role="columnheader">状态</span>
              </div>
              {results.map((result, index) => {
                const rowKey = `${result.package}-${result.source}-${index}`;
                const isCopied = copiedKey === rowKey;
                const installCmd = getInstallCommand(result);
                return (
                  <div className="result-row" role="row" key={rowKey}>
                    <strong role="cell">{result.package}</strong>
                    <span role="cell" className="source-cell-with-copy">
                      <span className={`source-tag ${result.source}`}>
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
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </section>

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
                依赖关系图谱
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
                依赖清单列表
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
          <button
            className="button ghost"
            style={{
              marginRight: "16px",
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
        <div className="log-console">
          {logs.length ? (
            logs.map((line, index) => (
              <div key={`${line}-${index}`}>
                <span>{String(index + 1).padStart(2, "0")}</span>
                {line}
              </div>
            ))
          ) : (
            <EmptyState text="日志将在检索开始后显示" />
          )}
        </div>
      </section>
    </div>
  );
}
