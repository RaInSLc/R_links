import { PanelHeader, Toggle } from "./components";
import { MAX_RESULT_FIELD_CHARS, MAX_TOKEN_CHARS, type MirrorSpeedResult, type NetworkDiagnostic } from "./utils";
import { mirrors, type Settings } from "./types";

interface Props {
  settings: Settings;
  tokenConfigured: boolean;
  showToken: boolean;
  settingsBusy: boolean;
  speedTesting: boolean;
  speedResults: MirrorSpeedResult[];
  networkDiagnostics: NetworkDiagnostic[];
  diagnosticsBusy: boolean;
  onProxyChange: (value: string) => void;
  onTokenChange: (value: string) => void;
  onTokenToggle: () => void;
  onClearToken: () => void;
  onFullSearchChange: (value: boolean) => void;
  onSearchConcurrencyChange: (value: number) => void;
  onCranMirrorChange: (value: string) => void;
  onRLibPathChange: (value: string) => void;
  onMirrorSelect: (value: string) => void;
  onSaveSettings: () => void;
  onTestSpeed: () => void;
  onTestNetwork: () => void;
}

export function SettingsNetworkPanel({
  settings,
  tokenConfigured,
  showToken,
  settingsBusy,
  speedTesting,
  speedResults,
  networkDiagnostics,
  diagnosticsBusy,
  onProxyChange,
  onTokenChange,
  onTokenToggle,
  onClearToken,
  onFullSearchChange,
  onSearchConcurrencyChange,
  onCranMirrorChange,
  onRLibPathChange,
  onMirrorSelect,
  onSaveSettings,
  onTestSpeed,
  onTestNetwork,
}: Props) {
  return <>
    <section className="panel settings-panel">
      <PanelHeader step="网络" title="连接设置" meta="独立配置" />
      <label className="field">
        <span>网络代理</span><small>支持 127.0.0.1:7890 或无凭据代理 URL，不允许路径或查询参数</small>
        <input value={settings.proxy} onChange={(event) => onProxyChange(event.currentTarget.value)} placeholder="不使用代理" maxLength={MAX_RESULT_FIELD_CHARS} />
        <small style={{ display: "block", marginTop: "6px" }}>当前代理：{settings.proxy.trim() ? settings.proxy : "未配置（使用系统/直连）"}；代理是否生效请使用下方网络诊断验证。</small>
        <button className="button ghost" type="button" onClick={onTestNetwork} disabled={diagnosticsBusy} style={{ marginTop: "9px" }}>{diagnosticsBusy ? "正在诊断..." : "诊断代理与网络"}</button>
        {networkDiagnostics.length > 0 && <div style={{ marginTop: "10px", fontSize: "12px" }}>{networkDiagnostics.map((item) => <div key={item.target} style={{ borderTop: "1px solid var(--line)", padding: "7px 0" }}><strong>{item.target}</strong>：{item.success ? `成功 ${item.statusCode ?? ""}，${item.latencyMs}ms` : `失败${item.statusCode ? ` HTTP ${item.statusCode}` : ""}`}<small style={{ display: "block", color: item.success ? "var(--muted)" : "#b91c1c" }}>{item.error || `代理 ${item.proxy}`}</small></div>)}</div>}
      </label>
      <label className="field">
        <span>GitHub Token</span><small>{tokenConfigured ? "已保存 Token；留空保存会继续保留现有 Token" : "仅保存在本应用的数据目录，用于提高 API 配额"}</small>
        <div className="secret-field"><input type={showToken ? "text" : "password"} value={settings.githubToken} onChange={(event) => onTokenChange(event.currentTarget.value)} placeholder="ghp_..." autoComplete="off" spellCheck={false} maxLength={MAX_TOKEN_CHARS} /><button type="button" onClick={onTokenToggle}>{showToken ? "隐藏" : "显示"}</button></div>
        {tokenConfigured && !settings.githubToken.trim() && <button type="button" className="text-button danger-text" onClick={onClearToken} disabled={settingsBusy}>清除已保存 Token</button>}
      </label>
      <Toggle checked={settings.fullSearch} label="全量检索" description="命中 CRAN 或 Bioconductor 后仍继续查询 GitHub" onChange={onFullSearchChange} />
      <div className="field" style={{ margin: "0 17px", marginTop: "12px" }}><span>搜索并发上限</span><small>同时检索的包数量（允许 1 到 12，默认 6；较高值更快但更容易触发限流）</small><input type="number" aria-label="搜索并发上限" min={1} max={12} value={settings.searchConcurrency} onChange={(event) => { const value = Number(event.currentTarget.value); if (Number.isFinite(value) && value >= 1 && value <= 12) onSearchConcurrencyChange(Math.floor(value)); }} /></div>
    </section>
    <section className="panel settings-panel">
      <PanelHeader step="镜像" title="CRAN 镜像" meta="实时影响脚本" />
      <div className="mirror-list">{mirrors.map((mirror) => <button key={mirror.value} className={settings.cranMirror === mirror.value ? "selected" : ""} aria-pressed={settings.cranMirror === mirror.value} onClick={() => onMirrorSelect(mirror.value)}><span>{mirror.label}</span><code>{mirror.value}</code></button>)}</div>
      <label className="field compact"><span>自定义镜像</span><input value={settings.cranMirror} onChange={(event) => onCranMirrorChange(event.currentTarget.value)} placeholder="https://cloud.r-project.org" maxLength={MAX_RESULT_FIELD_CHARS} /><small>使用 Posit Package Manager（RSPM）时，生成脚本会启用 R 的 binary 包类型；R 会按当前操作系统、架构和 R 版本选择预编译包，普通 CRAN 镜像仍保持默认行为。</small></label>
      <label className="field compact"><span>R 库路径（可选）</span><input value={settings.rLibPath} onChange={(event) => onRLibPathChange(event.currentTarget.value)} placeholder="留空使用当前 R 默认库路径" maxLength={MAX_RESULT_FIELD_CHARS} /><small>指定后写入安装命令的 <code>lib</code> 参数，并自动创建目录；适合多 R 版本或项目级隔离。</small></label>
      <button className="button primary save-button" onClick={() => onSaveSettings()} disabled={settingsBusy}>{settingsBusy ? "处理中..." : "保存设置"}</button>
      <div className="field" style={{ margin: "0 17px", marginTop: "12px" }}><span>镜像速度测试</span><small>并发请求各镜像的 PACKAGES.gz 响应时间</small><button className="button ghost" onClick={onTestSpeed} disabled={speedTesting} style={{ marginTop: "9px" }}>{speedTesting ? "正在测速..." : "测速"}</button>
        {speedResults.length > 0 && !speedTesting && speedResults[0]?.success && <div className="apply-fastest-bar"><span>最快镜像：{speedResults[0].label} ({speedResults[0].latencyMs}ms)</span><button type="button" className="button" onClick={() => onMirrorSelect(speedResults[0].mirror)} disabled={settings.cranMirror === speedResults[0].mirror}>{settings.cranMirror === speedResults[0].mirror ? "已应用 ✓" : "应用最快镜像"}</button></div>}
        {speedResults.length > 0 && <div style={{ marginTop: "10px", fontSize: "13px" }}>{speedResults.map((result, index) => { const color = !result.success ? "#dc2626" : result.latencyMs < 500 ? "#16a34a" : result.latencyMs < 1500 ? "#d97706" : "#dc2626"; return <div key={index} style={{ display: "flex", gap: "8px", padding: "6px 0", alignItems: "center", borderBottom: "1px solid var(--line)" }}><span style={{ fontWeight: 600, minWidth: "100px" }}>{result.label}</span><span style={{ minWidth: "70px", fontWeight: 600, color, padding: "2px 6px", borderRadius: "4px", background: `${color}15` }}>{result.success ? `${result.latencyMs}ms` : "失败"}{index === 0 && result.success && <span style={{ marginLeft: "4px", fontSize: "11px" }}>最快</span>}</span><span style={{ fontSize: "12px", color: "var(--muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", flex: 1 }}>{result.error || result.mirror}</span>{index === 0 && result.success && result.mirror && <button type="button" className="button ghost" onClick={() => onMirrorSelect(result.mirror)}>使用此镜像</button>}</div>; })}</div>}
      </div>
    </section>
  </>;
}
