import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { sourceNames } from "./types";
import {
  diagnoseDependencyGraph,
  type DependencyGraph,
  type DependencyNode,
  type ReverseDependenciesInfo,
} from "./utils";

export function DependencyGraphView({ graph }: { graph: DependencyGraph }) {
  const [hoveredNode, setHoveredNode] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<DependencyNode | null>(null);
  const [filterStrength, setFilterStrength] = useState<"all" | "heavy">("all");
  const [depSearch, setDepSearch] = useState("");
  const [reverseDeps, setReverseDeps] = useState<ReverseDependenciesInfo | null>(null);
  const [reverseDepsLoading, setReverseDepsLoading] = useState(false);
  const fetchDepsToken = useRef(0);

  useEffect(() => {
    setHoveredNode(null);
    setSelectedNode(null);
    setReverseDeps(null);
    setReverseDepsLoading(false);
    fetchDepsToken.current += 1;
  }, [graph]);

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
      if (token === fetchDepsToken.current) setReverseDepsLoading(false);
    }
  }

  const filteredEdges = useMemo(
    () => filterStrength === "heavy" ? graph.edges.filter((edge) => edge.strength === "heavy") : graph.edges,
    [graph.edges, filterStrength],
  );

  const filteredNodes = useMemo(() => {
    if (filterStrength !== "heavy") return graph.nodes;
    const activePackages = new Set<string>(graph.roots);
    filteredEdges.forEach((edge) => {
      activePackages.add(edge.from);
      activePackages.add(edge.to);
    });
    return graph.nodes.filter((node) => activePackages.has(node.package));
  }, [graph.nodes, graph.roots, filteredEdges, filterStrength]);

  const { layoutNodes, svgWidth, svgHeight } = useMemo(() => {
    const levels: Record<number, string[]> = {};
    filteredNodes.forEach((node) => {
      const depth = node.depth;
      if (!levels[depth]) levels[depth] = [];
      levels[depth].push(node.package);
    });
    const maxDepth = Math.max(...filteredNodes.map((node) => node.depth), 0);
    const colWidth = 220;
    const svgWidth = Math.max(700, (maxDepth + 1) * colWidth + 100);
    const maxNodesInLevel = Math.max(...Object.values(levels).map((items) => items.length), 1);
    const nodeSpacing = 48;
    const svgHeight = Math.max(450, maxNodesInLevel * nodeSpacing + 60);
    const positions: Record<string, { x: number; y: number }> = {};
    Object.keys(levels).forEach((depthText) => {
      const depth = Number(depthText);
      const packages = levels[depth];
      packages.forEach((packageName, index) => {
        const x = 50 + depth * colWidth;
        const y = packages.length === 1
          ? svgHeight / 2
          : (svgHeight - (packages.length - 1) * nodeSpacing) / 2 + index * nodeSpacing;
        positions[packageName] = { x, y };
      });
    });
    return { layoutNodes: positions, svgWidth, svgHeight };
  }, [filteredNodes]);

  const highlightedNodes = useMemo(() => {
    if (!hoveredNode) return null;
    const related = new Set<string>([hoveredNode]);
    filteredEdges.forEach((edge) => {
      if (edge.from === hoveredNode) related.add(edge.to);
      if (edge.to === hoveredNode) related.add(edge.from);
    });
    return related;
  }, [hoveredNode, filteredEdges]);

  return (
    <div className="dep-graph-container">
      <div className="dep-graph-toolbar" style={{ display: "flex", alignItems: "center", marginBottom: "12px" }}>
        <span className="toolbar-title" style={{ fontSize: "13px", fontWeight: "500", marginRight: "12px" }}>图谱过滤：</span>
        <button className={`button small ${filterStrength === "all" ? "primary" : "ghost"}`} onClick={() => setFilterStrength("all")} style={{ padding: "4px 10px", fontSize: "12px", height: "auto" }}>
          显示所有依赖 ({graph.summary.totalNodes})
        </button>
        <button className={`button small ${filterStrength === "heavy" ? "primary" : "ghost"}`} onClick={() => setFilterStrength("heavy")} style={{ padding: "4px 10px", fontSize: "12px", height: "auto", marginLeft: "8px" }}>
          仅重度依赖 ({graph.summary.heavyNodes})
        </button>
        <input type="text" value={depSearch} onChange={(event) => setDepSearch(event.target.value)} placeholder="搜索节点..." style={{ marginLeft: "auto", padding: "4px 8px", fontSize: "12px", width: "140px", borderRadius: "4px", border: "1px solid var(--line)", background: "var(--input-bg, #fff)", color: "var(--ink)" }} />
      </div>
      <div className="dep-graph-workspace" style={{ display: "flex", gap: "16px" }}>
        <div className="dep-graph-viewport" style={{ flex: 1, height: "480px", overflow: "auto", border: "1px solid var(--border-color, #e0e0e0)", borderRadius: "6px", backgroundColor: "var(--console-bg, #fafafa)", position: "relative" }}>
          <svg width={svgWidth} height={svgHeight} style={{ overflow: "visible" }}>
            <defs>
              <marker id="arrow-heavy" viewBox="0 0 10 10" refX="12" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#0066cc" /></marker>
              <marker id="arrow-light" viewBox="0 0 10 10" refX="12" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#b0b0b0" /></marker>
            </defs>
            {filteredEdges.map((edge, index) => {
              const from = layoutNodes[edge.from];
              const to = layoutNodes[edge.to];
              if (!from || !to) return null;
              const related = hoveredNode ? edge.from === hoveredNode || edge.to === hoveredNode : false;
              const strokeColor = edge.strength === "heavy"
                ? (hoveredNode ? (related ? "#0066cc" : "#e0e0e0") : "#a3c2e0")
                : (hoveredNode ? (related ? "#b0b0b0" : "#f0f0f0") : "#d0d0d0");
              const dx = to.x - from.x;
              return <path key={`edge-${index}`} d={`M ${from.x + 130} ${from.y} C ${from.x + dx / 2 + 65} ${from.y}, ${from.x + dx / 2 + 65} ${to.y}, ${to.x} ${to.y}`} fill="none" stroke={strokeColor} strokeWidth={edge.strength === "heavy" ? (related ? 2.5 : 1.5) : 1} strokeDasharray={edge.strength === "light" ? "3,3" : undefined} opacity={hoveredNode ? (related ? 1 : 0.2) : 0.8} markerEnd={edge.strength === "heavy" ? "url(#arrow-heavy)" : "url(#arrow-light)"} />;
            })}
            {filteredNodes.map((node) => {
              const position = layoutNodes[node.package];
              if (!position) return null;
              const highlighted = highlightedNodes ? highlightedNodes.has(node.package) : true;
              const isRoot = graph.roots.includes(node.package);
              const isShared = node.rootPackages.length > 1;
              const borderClass = isRoot ? "dep-node-root" : isShared ? "dep-node-shared" : node.status === "unresolved" ? "dep-node-unresolved" : "dep-node-normal";
              const matchesSearch = !depSearch.trim() || node.package.toLowerCase().includes(depSearch.trim().toLowerCase());
              return (
                <foreignObject key={`node-${node.package}`} x={position.x} y={position.y - 18} width="150" height="36" style={{ overflow: "visible", cursor: "pointer", opacity: matchesSearch ? (hoveredNode && !highlighted ? 0.3 : 1) : 0.15 }} onMouseEnter={() => setHoveredNode(node.package)} onMouseLeave={() => setHoveredNode(null)} onClick={() => { setSelectedNode(node); void fetchReverseDeps(node.package); }} onDoubleClick={async (event) => { event.preventDefault(); try { await writeText(node.package); } catch { /* clipboard is optional */ } }}>
                  <div className={`dep-node-card ${borderClass} ${hoveredNode === node.package ? "hovered" : ""}`}>
                    <span className="dep-node-title" title={node.package}>{node.package}</span>
                    <span className="dep-node-meta">{isRoot ? "根依赖包" : node.version !== "unknown" ? `v${node.version}` : "解析失败"}</span>
                    {isShared && <div className="dep-node-shared-badge" title="多根共享依赖">S</div>}
                  </div>
                </foreignObject>
              );
            })}
          </svg>
        </div>
        <div className="dep-graph-sidebar" style={{ width: "240px", border: "1px solid var(--border-color, #e0e0e0)", borderRadius: "6px", padding: "16px", backgroundColor: "var(--card-bg, #ffffff)", fontSize: "13px" }}>
          {selectedNode ? (
            <div>
              <h4 style={{ margin: "0 0 12px 0", color: "var(--primary-color)" }}>{selectedNode.package}</h4>
              <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                <div><strong>来源：</strong><span className={`source-tag ${selectedNode.source}`}>{sourceNames[selectedNode.source] ?? selectedNode.source}</span></div>
                <div><strong>版本：</strong><code>{selectedNode.version}</code></div>
                <div><strong>引入深度：</strong><code>{selectedNode.depth}</code></div>
                <div><strong>根包路径：</strong><div style={{ display: "flex", flexWrap: "wrap", gap: "4px", marginTop: "4px" }}>{selectedNode.rootPackages.map((root) => <span key={root} style={{ padding: "2px 6px", borderRadius: "3px", backgroundColor: "#f0f0f0", fontSize: "11px" }}>{root}</span>)}</div></div>
                <div><strong>子依赖数：</strong><code>{selectedNode.directDependencyCount}</code></div>
                <div><strong>重子依赖：</strong><code>{selectedNode.heavyDependencyCount}</code></div>
                <div><strong>解析状态：</strong><span style={{ color: selectedNode.status === "resolved" ? "green" : "red" }}>{selectedNode.status === "resolved" ? "已解析" : "解析失败"}</span></div>
              </div>
              <div style={{ marginTop: "12px", borderTop: "1px solid var(--line)", paddingTop: "10px" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "8px" }}><strong style={{ fontSize: "12px" }}>反向依赖分析 (CRAN)</strong><button className="button ghost" style={{ padding: "2px 8px", fontSize: "11px", height: "auto", minHeight: "auto" }} onClick={() => void fetchReverseDeps(selectedNode.package)} disabled={reverseDepsLoading}>{reverseDepsLoading ? "加载中..." : "刷新"}</button></div>
                {reverseDeps ? <div style={{ display: "flex", flexDirection: "column", gap: "4px", fontSize: "12px" }}><div><span style={{ color: "var(--muted)" }}>反向 Depends:</span> <strong>{reverseDeps.depends}</strong></div><div><span style={{ color: "var(--muted)" }}>反向 Imports:</span> <strong>{reverseDeps.imports}</strong></div><div><span style={{ color: "var(--muted)" }}>反向 Suggests:</span> <strong>{reverseDeps.suggests}</strong></div><div><span style={{ color: "var(--muted)" }}>反向 LinkingTo:</span> <strong>{reverseDeps.linkingTo}</strong></div></div> : reverseDepsLoading ? <span style={{ color: "var(--muted)", fontSize: "12px" }}>正在查询 CRAN...</span> : <span style={{ color: "var(--muted)", fontSize: "12px" }}>非 CRAN 包或查询失败</span>}
              </div>
            </div>
          ) : <div style={{ color: "#999", textAlign: "center", paddingTop: "60px" }}>点击图谱节点查看详细信息</div>}
        </div>
      </div>
    </div>
  );
}

export function DependencyListView({ graph }: { graph: DependencyGraph }) {
  const [searchTerm, setSearchTerm] = useState("");
  const [filterType, setFilterType] = useState<"all" | "heavy" | "light" | "shared">("all");
  const diagnostics = diagnoseDependencyGraph(graph);
  const filteredNodes = useMemo(() => graph.nodes.filter((node) => {
    const matchesSearch = node.package.toLowerCase().includes(searchTerm.toLowerCase());
    const isShared = node.rootPackages.length > 1;
    const isLight = graph.edges.some((edge) => edge.to === node.package && edge.strength === "light") && !graph.roots.includes(node.package);
    const isHeavy = !isLight;
    if (filterType === "heavy") return matchesSearch && isHeavy;
    if (filterType === "light") return matchesSearch && isLight;
    if (filterType === "shared") return matchesSearch && isShared;
    return matchesSearch;
  }), [graph.nodes, graph.edges, graph.roots, searchTerm, filterType]);

  return (
    <div className="dep-list-container">
      {diagnostics.length > 0 && <div className="notice warning" style={{ marginBottom: "12px" }}><strong>依赖诊断</strong>{diagnostics.map((diagnostic) => <div key={`${diagnostic.type}-${diagnostic.packages.join("-")}`}>{diagnostic.detail}</div>)}</div>}
      <div className="dep-list-filters" style={{ display: "flex", gap: "8px", marginBottom: "16px", alignItems: "center", flexWrap: "wrap" }}>
        <input type="text" placeholder="搜索依赖包名..." value={searchTerm} onChange={(event) => setSearchTerm(event.target.value)} className="input" style={{ width: "180px", padding: "4px 8px", fontSize: "13px" }} />
        {(["all", "heavy", "light", "shared"] as const).map((kind) => <button key={kind} className={`button small ${filterType === kind ? "primary" : "ghost"}`} onClick={() => setFilterType(kind)} style={{ padding: "4px 10px", fontSize: "12px", height: "auto" }}>{kind === "all" ? `全部 (${graph.nodes.length})` : kind === "heavy" ? `重依赖 (${graph.summary.heavyNodes})` : kind === "light" ? `轻依赖 (${graph.summary.lightNodes})` : `共享依赖 (${graph.summary.sharedNodes})`}</button>)}
      </div>
      <div className="result-table" role="table" aria-label="依赖包清单">
        <div className="result-row result-head" role="row"><span role="columnheader">依赖包名</span><span role="columnheader">依赖关系</span><span role="columnheader">引入自 (根包)</span><span role="columnheader">最新版本</span><span role="columnheader">子依赖数量</span></div>
        {filteredNodes.length === 0 ? <div style={{ textAlign: "center", padding: "20px", color: "#999" }}>没有匹配的依赖包</div> : filteredNodes.map((node, index) => {
          const isShared = node.rootPackages.length > 1;
          const isRoot = graph.roots.includes(node.package);
          return <div className="result-row" role="row" key={`${node.package}-${index}`}><strong role="cell">{node.package}{isShared && <span className="source-tag" style={{ marginLeft: "8px", backgroundColor: "#ebd6ff", color: "#6600cc", fontSize: "10px", padding: "2px 4px" }}>共享依赖</span>}</strong><span role="cell">{isRoot ? "根包" : `深度 ${node.depth}`}</span><span role="cell" style={{ fontSize: "12px" }}>{node.rootPackages.join(", ")}</span><code role="cell">{node.version}</code><span role="cell">{node.directDependencyCount} 个 (重: {node.heavyDependencyCount})</span></div>;
        })}
      </div>
    </div>
  );
}
