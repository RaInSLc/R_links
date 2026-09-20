import { useState, type ChangeEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CacheSettingsPanel, type PackageCacheEntry } from "./CacheSettingsPanel";
import { mirrors, type InputRules, type Settings } from "./types";
import type { MirrorSpeedResult, NetworkDiagnostic, ToolchainCheck } from "./utils";
import { SettingsNetworkPanel } from "./SettingsNetworkPanel";
import { SettingsStrategyPanel } from "./SettingsStrategyPanel";
import { InputRulesPanel } from "./InputRulesPanel";
import { SettingsAppearancePanel } from "./SettingsAppearancePanel";
import { SettingsBackupPanel } from "./SettingsBackupPanel";

export interface SettingsViewProps {
  settings: Settings; tokenConfigured: boolean; showToken: boolean; settingsBusy: boolean; currentTheme: string; currentFont: string;
  checkingUpdate: boolean; updateState: "idle" | "checking" | "available" | "downloading" | "installing" | "readyToRestart" | "upToDate" | "error"; updateMessage: string; appVersion: string; updateVersion: string;
  onProxyChange: (value: string) => void;
  onTokenChange: (value: string) => void;
  onTokenToggle: () => void;
  onClearToken: () => void;
  onFullSearchChange: (value: boolean) => void;
  onSearchConcurrencyChange: (value: number) => void;
  onArchiveGithubMajorGapChange: (value: number) => void;
  onConditionalChange: (value: boolean) => void;
  onInstallDependenciesChange: (value: boolean) => void;
  onShowRemoteVersionChange: (value: boolean) => void;
  onUseCacheChange: (value: boolean) => void;
  onUseFilterChange: (value: boolean) => void;
  onMaxCacheEntriesChange: (value: number) => void;
  onCranMirrorChange: (value: string) => void;
  onRLibPathChange: (value: string) => void;
  onResolveDependenciesChange: (value: boolean) => void;
  onMaxDependencyDepthChange: (value: number) => void;
  onIncludeLightDependenciesChange: (value: boolean) => void;
  onMaxDependencyNodesChange: (value: number) => void;
  onMirrorSelect: (value: string) => void;
  onSaveSettings: () => void;
  onReplaceSettings: (value: Settings) => void;
  onThemeChange: (value: string) => void;
  onFontChange: (value: string) => void;
  currentFontSize: number;
  onFontSizeChange: (value: number) => void;
  onCheckUpdates: () => void;
  onClearCache: () => Promise<void>;
  onExportDiagnostics: () => Promise<void>;
  inputRules: InputRules;
  onInputRulesChange: (value: InputRules) => void;
  onReplaceInputRules: (value: InputRules) => void;
  onSaveInputRules: () => void;
  inputRulesBusy: boolean;
}

type MenuKey = "network" | "strategy" | "cache" | "input" | "appearance" | "backup";
const menus: Array<{ key: MenuKey; label: string; meta: string }> = [
  { key: "network", label: "网络连接", meta: "代理、Token、CRAN 镜像" }, { key: "strategy", label: "检索策略", meta: "安装默认值、依赖图" }, { key: "cache", label: "缓存", meta: "包结果缓存、诊断" }, { key: "input", label: "输入过滤", meta: "分隔、注释、排除规则" }, { key: "appearance", label: "界面与系统", meta: "主题、字号、更新" }, { key: "backup", label: "配置备份", meta: "导出、导入、恢复" },
];

export function SettingsView(props: SettingsViewProps) {
  const [activeMenu, setActiveMenu] = useState<MenuKey>("network");
  const [speedTesting, setSpeedTesting] = useState(false); const [speedResults, setSpeedResults] = useState<MirrorSpeedResult[]>([]);
  const [networkDiagnostics, setNetworkDiagnostics] = useState<NetworkDiagnostic[]>([]); const [diagnosticsBusy, setDiagnosticsBusy] = useState(false);
  const [toolchainBusy, setToolchainBusy] = useState(false); const [toolchainChecks, setToolchainChecks] = useState<ToolchainCheck[]>([]);
  const [cacheEntries, setCacheEntries] = useState<PackageCacheEntry[]>([]); const [cacheBusy, setCacheBusy] = useState(false);
  const testSpeed = async () => { setSpeedTesting(true); setSpeedResults([]); try { const urls = [...new Set([...mirrors.map((item) => item.value), props.settings.cranMirror].filter((url) => url.trim()))]; const results = await invoke<MirrorSpeedResult[]>("test_mirror_speed", { mirrorUrls: urls }); setSpeedResults(results.sort((a, b) => Number(b.success) - Number(a.success) || a.latencyMs - b.latencyMs)); } catch (error) { setSpeedResults([{ mirror: "", label: "测速失败", latencyMs: 0, success: false, error: String(error instanceof Error ? error.message : error) }]); } finally { setSpeedTesting(false); } };
  const testNetwork = async () => { setDiagnosticsBusy(true); setNetworkDiagnostics([]); try { setNetworkDiagnostics(await invoke<NetworkDiagnostic[]>("test_network_connection")); } catch (error) { setNetworkDiagnostics([{ target: "诊断失败", url: "", success: false, latencyMs: 0, proxy: props.settings.proxy || "未配置", error: String(error instanceof Error ? error.message : error) }]); } finally { setDiagnosticsBusy(false); } };
  const checkToolchain = async () => { setToolchainBusy(true); setToolchainChecks([]); try { setToolchainChecks(await invoke<ToolchainCheck[]>("check_system_toolchain")); } catch (error) { setToolchainChecks([{ tool: "Doctor", available: false, version: "", advice: `检测失败：${String(error)}` }]); } finally { setToolchainBusy(false); } };
  const loadCache = async () => { setCacheBusy(true); try { setCacheEntries(await invoke<PackageCacheEntry[]>("load_package_cache")); } catch { setCacheEntries([]); } finally { setCacheBusy(false); } };
  const deleteCache = async (entry: PackageCacheEntry) => { setCacheBusy(true); try { await invoke("delete_package_cache_entry", { package: entry.packageName, source: entry.source, version: entry.version, repository: entry.repository, realName: entry.realName }); setCacheEntries((current) => current.filter((item) => item !== entry)); } finally { setCacheBusy(false); } };
  const clearInvalidated = async () => { setCacheBusy(true); try { await invoke("clear_invalidated_cache"); await loadCache(); } finally { setCacheBusy(false); } };
  const exportCache = async () => { setCacheBusy(true); try { const content = await invoke<string>("export_package_cache"); const url = URL.createObjectURL(new Blob([content], { type: "application/json" })); const anchor = document.createElement("a"); anchor.href = url; anchor.download = `rlinks_cache_${new Date().toISOString().slice(0, 10)}.json`; document.body.appendChild(anchor); anchor.click(); document.body.removeChild(anchor); URL.revokeObjectURL(url); } finally { setCacheBusy(false); } };
  const importCache = async (event: ChangeEvent<HTMLInputElement>) => { const file = event.target.files?.[0]; event.target.value = ""; if (!file) return; setCacheBusy(true); try { const count = await invoke<number>("import_package_cache", { content: await file.text() }); await loadCache(); window.alert(`已合并 ${count} 条缓存记录`); } catch (error) { window.alert(`缓存导入失败：${String(error)}`); } finally { setCacheBusy(false); } };
  const body = activeMenu === "network" ? <SettingsNetworkPanel {...props} speedTesting={speedTesting} speedResults={speedResults} networkDiagnostics={networkDiagnostics} diagnosticsBusy={diagnosticsBusy} onTestSpeed={() => void testSpeed()} onTestNetwork={() => void testNetwork()} /> : activeMenu === "strategy" ? <SettingsStrategyPanel {...props} /> : activeMenu === "input" ? <InputRulesPanel {...props} /> : activeMenu === "appearance" ? <SettingsAppearancePanel {...props} /> : activeMenu === "backup" ? <SettingsBackupPanel {...props} /> : <CacheSettingsPanel settings={props.settings} cacheEntries={cacheEntries} cacheBusy={cacheBusy} toolchainBusy={toolchainBusy} toolchainChecks={toolchainChecks} onLoadCache={() => void loadCache()} onClearInvalidated={() => void clearInvalidated()} onExportCache={() => void exportCache()} onImportCache={(event) => void importCache(event)} onDeleteCacheEntry={(entry) => void deleteCache(entry)} onCheckToolchain={() => void checkToolchain()} onUseCacheChange={props.onUseCacheChange} onMaxCacheEntriesChange={props.onMaxCacheEntriesChange} onClearCache={props.onClearCache} onExportDiagnostics={props.onExportDiagnostics} />;
  return <div className="settings-shell"><aside className="settings-menu" aria-label="设置分类菜单">{menus.map((item) => <button key={item.key} type="button" className={activeMenu === item.key ? "active" : ""} aria-pressed={activeMenu === item.key} onClick={() => setActiveMenu(item.key)}><strong>{item.label}</strong><small>{item.meta}</small></button>)}</aside><div className="settings-layout">{body}</div></div>;
}
