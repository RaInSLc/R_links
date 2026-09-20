import { useRef, type ChangeEvent } from "react";
import { PanelHeader, Toggle } from "./components";
import type { Settings } from "./types";

export interface PackageCacheEntry {
  packageName: string;
  source: string;
  version: string;
  repository: string;
  realName: string;
  cachedAt: string;
  verifiedCount: number;
  upVotes: number;
  downVotes: number;
  invalidated: boolean;
}

interface CacheSettingsPanelProps {
  settings: Settings;
  cacheEntries: PackageCacheEntry[];
  cacheBusy: boolean;
  toolchainBusy: boolean;
  toolchainChecks: Array<{ tool: string; available: boolean; version: string; advice: string }>;
  onLoadCache: () => void;
  onClearInvalidated: () => void;
  onExportCache: () => void;
  onImportCache: (event: ChangeEvent<HTMLInputElement>) => void;
  onDeleteCacheEntry: (entry: PackageCacheEntry) => void;
  onCheckToolchain: () => void;
  onUseCacheChange: (value: boolean) => void;
  onMaxCacheEntriesChange: (value: number) => void;
  onClearCache: () => Promise<void>;
  onExportDiagnostics: () => Promise<void>;
}

export function CacheSettingsPanel({
  settings,
  cacheEntries,
  cacheBusy,
  toolchainBusy,
  toolchainChecks,
  onLoadCache,
  onClearInvalidated,
  onExportCache,
  onImportCache,
  onDeleteCacheEntry,
  onCheckToolchain,
  onUseCacheChange,
  onMaxCacheEntriesChange,
  onClearCache,
  onExportDiagnostics,
}: CacheSettingsPanelProps) {
  const fileCacheRef = useRef<HTMLInputElement>(null);
  return (
    <section className="panel settings-panel">
      <PanelHeader step="缓存" title="包结果缓存" meta="避免重复检索" />
      <div className="field" style={{ margin: "0 17px", marginTop: "12px" }}>
        <span>缓存条目</span>
        <small>显示当前有效缓存；失效条目不会参与离线命中。缓存默认保留 7 天。</small>
        <div style={{ display: "flex", gap: "8px", alignItems: "center", marginTop: "9px" }}>
          <button className="button ghost" onClick={onLoadCache} disabled={cacheBusy}>{cacheBusy ? "处理中..." : "刷新列表"}</button>
          <button className="button ghost danger-text" onClick={onClearInvalidated} disabled={cacheBusy}>清理失效项</button>
          <button className="button ghost" onClick={onExportCache} disabled={cacheBusy}>导出共享缓存</button>
          <button className="button ghost" onClick={() => fileCacheRef.current?.click()} disabled={cacheBusy}>导入共享缓存</button>
          <input ref={fileCacheRef} type="file" accept=".json" onChange={onImportCache} style={{ display: "none" }} />
          <span style={{ color: "var(--muted)", fontSize: "12px" }}>{cacheEntries.length} 条</span>
        </div>
        {cacheEntries.length > 0 && <div style={{ marginTop: "10px" }}>{cacheEntries.map((entry) => <div key={`${entry.packageName}-${entry.source}-${entry.version}-${entry.repository}-${entry.realName}`} style={{ display: "flex", gap: "8px", alignItems: "center", borderBottom: "1px solid var(--line)", padding: "8px 0" }}><div style={{ minWidth: 0, flex: 1 }}><strong>{entry.packageName}</strong><small style={{ display: "block" }}>{entry.source} · {entry.version || "未知版本"} · {entry.invalidated ? "已失效" : entry.verifiedCount > 0 ? `已验证 ${entry.verifiedCount} 次` : "未验证"}</small></div><button className="button ghost" onClick={() => onDeleteCacheEntry(entry)} disabled={cacheBusy}>删除</button></div>)}</div>}
      </div>
      <div className="field" style={{ margin: "0 17px", marginTop: "12px" }}>
        <span>R 编译环境 Doctor</span>
        <small>只读检查 R、Rscript、Git 和当前系统的源码包编译工具，不会安装或修改任何系统组件。</small>
        <button className="button ghost" type="button" onClick={onCheckToolchain} disabled={toolchainBusy} style={{ marginTop: "9px" }}>{toolchainBusy ? "正在检查..." : "检查编译环境"}</button>
        {toolchainChecks.length > 0 && <div style={{ marginTop: "10px", fontSize: "12px" }}>{toolchainChecks.map((item) => <div key={item.tool} style={{ borderTop: "1px solid var(--line)", padding: "7px 0" }}><strong>{item.available ? "✓" : "!"} {item.tool}</strong><small style={{ display: "block", color: item.available ? "var(--muted)" : "#b91c1c" }}>{item.available ? item.version.split("\n")[0] : item.advice}</small></div>)}</div>}
      </div>
      <div className="toggle-row" style={{ flexDirection: "column", gap: "4px", padding: "4px 17px" }}><Toggle checked={settings.useCache} label="使用包结果缓存" description="启用后命中缓存的包跳过在线检索；关闭后每次均在线重新检索" onChange={onUseCacheChange} /></div>
      <div className="field" style={{ margin: "0 17px", marginTop: "12px" }}>
        <span>最大缓存条数</span><small>缓存保留的最大条数限制，允许范围：1 至 10000 条</small>
        <input type="number" min={1} max={10000} value={settings.maxCacheEntries} onChange={(event) => { const value = parseInt(event.currentTarget.value, 10); onMaxCacheEntriesChange(Number.isNaN(value) ? 1000 : value); }} />
        <div className="cache-progress-bar"><div className="cache-progress-fill" style={{ width: `${Math.min(100, (settings.maxCacheEntries / 10000) * 100)}%` }} /><span className="cache-progress-label">{settings.maxCacheEntries} / 10000 条 · {settings.maxCacheEntries >= 5000 ? "高容量" : settings.maxCacheEntries >= 1000 ? "标准容量" : "精简容量"}</span></div>
      </div>
      <div className="field" style={{ margin: "0 17px", marginTop: "12px" }}><span>清理缓存数据</span><small>已缓存的包将跳过在线检索直接使用历史结果；清除后所有包都会重新在线检索</small><div style={{ display: "flex", gap: "8px", alignItems: "center", marginTop: "9px" }}><button className="button ghost" onClick={() => void onClearCache()}>清除缓存</button><button className="button ghost" onClick={() => void onExportDiagnostics()}>导出诊断</button></div></div>
    </section>
  );
}
