import { PanelHeader, Toggle } from "./components";
import { MAX_RESULT_FIELD_CHARS, MAX_TOKEN_CHARS } from "./utils";
import { mirrors } from "./types";
import type { Settings } from "./types";

interface SettingsViewProps {
  settings: Settings;
  tokenConfigured: boolean;
  showToken: boolean;
  settingsBusy: boolean;
  currentTheme: string;
  currentFont: string;
  checkingUpdate: boolean;
  updateMessage: string;
  onProxyChange: (value: string) => void;
  onTokenChange: (value: string) => void;
  onTokenToggle: () => void;
  onClearToken: () => void;
  onFullSearchChange: (v: boolean) => void;
  onCranMirrorChange: (value: string) => void;
  onMirrorSelect: (value: string) => void;
  onSaveSettings: () => void;
  onThemeChange: (theme: string) => void;
  onFontChange: (font: string) => void;
  onCheckUpdates: () => void;
  onClearCache: () => Promise<void>;
  onExportDiagnostics: () => Promise<void>;
}

export function SettingsView({
  settings, tokenConfigured, showToken, settingsBusy,
  currentTheme, currentFont, checkingUpdate, updateMessage,
  onProxyChange, onTokenChange, onTokenToggle, onClearToken,
  onFullSearchChange, onCranMirrorChange, onMirrorSelect,
  onSaveSettings, onThemeChange, onFontChange,
  onCheckUpdates, onClearCache, onExportDiagnostics,
}: SettingsViewProps) {
  return (
    <div className="settings-layout">
      <section className="panel settings-panel">
        <PanelHeader step="网络" title="连接设置" meta="独立配置" />
        <label className="field">
          <span>网络代理</span>
          <small>支持 127.0.0.1:7890 或无凭据代理 URL，不允许路径或查询参数</small>
          <input
            value={settings.proxy}
            onChange={(event) => onProxyChange(event.currentTarget.value)}
            placeholder="不使用代理"
            maxLength={MAX_RESULT_FIELD_CHARS}
          />
        </label>
        <label className="field">
          <span>GitHub Token</span>
          <small>
            {tokenConfigured
              ? "已保存 Token；留空保存会继续保留现有 Token"
              : "仅保存在本应用的数据目录，用于提高 API 配额"}
          </small>
          <div className="secret-field">
            <input
              type={showToken ? "text" : "password"}
              value={settings.githubToken}
              onChange={(event) => onTokenChange(event.currentTarget.value)}
              placeholder="ghp_..."
              autoComplete="off"
              spellCheck={false}
              maxLength={MAX_TOKEN_CHARS}
            />
            <button type="button" onClick={onTokenToggle}>
              {showToken ? "隐藏" : "显示"}
            </button>
          </div>
          {tokenConfigured && !settings.githubToken.trim() && (
            <button type="button" className="text-button danger-text" onClick={onClearToken} disabled={settingsBusy}>
              清除已保存 Token
            </button>
          )}
        </label>
        <Toggle
          checked={settings.fullSearch}
          label="全量检索"
          description="命中 CRAN 或 Bioconductor 后仍继续查询 GitHub"
          onChange={onFullSearchChange}
        />
        <div style={{ borderTop: "1px solid var(--line)", marginTop: "20px", paddingTop: "12px" }}>
          <div className="field" style={{ margin: "0 17px" }}>
            <span>界面风格</span>
            <small>选择您偏好的系统色彩，切换实时生效</small>
            <div className="theme-selector">
              {(["office", "green", "graphite"] as const).map((theme) => (
                <button
                  key={theme}
                  type="button"
                  className={`theme-card ${currentTheme === theme ? "selected" : ""}`}
                  onClick={() => onThemeChange(theme)}
                >
                  <div className="theme-preview-dots">
                    {theme === "office" && (<><div className="theme-dot" style={{ background: "#0f172a" }} /><div className="theme-dot" style={{ background: "#0f4c81" }} /><div className="theme-dot" style={{ background: "#e6f0fa" }} /></>)}
                    {theme === "green" && (<><div className="theme-dot" style={{ background: "#112c24" }} /><div className="theme-dot" style={{ background: "#176b4d" }} /><div className="theme-dot" style={{ background: "#dcece4" }} /></>)}
                    {theme === "graphite" && (<><div className="theme-dot" style={{ background: "#212529" }} /><div className="theme-dot" style={{ background: "#495057" }} /><div className="theme-dot" style={{ background: "#f1f3f5" }} /></>)}
                  </div>
                  <span>{theme === "office" ? "商务办公蓝" : theme === "green" ? "墨绿林野" : "石墨暗灰"}</span>
                </button>
              ))}
            </div>
          </div>
          <div className="field" style={{ margin: "0 17px", marginTop: "24px" }}>
            <span>字体风格</span>
            <small>选择最适合您显示器的排版</small>
            <div className="theme-selector">
              {(["modern", "system", "classic"] as const).map((font) => (
                <button
                  key={font}
                  type="button"
                  className={`theme-card ${currentFont === font ? "selected" : ""}`}
                  onClick={() => onFontChange(font)}
                >
                  <div className="theme-preview-dots" style={{ alignItems: 'center', justifyContent: 'center' }}>
                    <span style={{
                      fontFamily: font === "modern" ? "'Inter', 'Noto Sans SC', sans-serif" :
                        font === "system" ? '"Segoe UI", "Microsoft YaHei UI", sans-serif' :
                          '"SimSun", "宋体", serif',
                      fontSize: '15px', fontWeight: 600, color: 'var(--ink)'
                    }}>Aa</span>
                  </div>
                  <span>{font === "modern" ? "现代 (推荐)" : font === "system" ? "系统默认" : "传统宋体"}</span>
                </button>
              ))}
            </div>
          </div>
        </div>
      </section>

      <section className="panel settings-panel">
        <PanelHeader step="系统" title="应用更新" meta="版本维护" />
        <div className="field">
          <span>检查应用更新</span>
          <small>检查并安装最新版本的 R Package Command Center</small>
          <div style={{ display: 'flex', gap: '8px', alignItems: 'center', marginTop: '9px' }}>
            <button className="button primary" onClick={onCheckUpdates} disabled={checkingUpdate} style={{ marginLeft: 0 }}>
              {checkingUpdate ? '正在处理...' : '检查更新'}
            </button>
            {updateMessage && <span style={{fontSize: '14px', color: 'var(--muted)'}}>{updateMessage}</span>}
          </div>
        </div>
      </section>

      <section className="panel settings-panel">
        <PanelHeader step="缓存" title="包结果缓存" meta="避免重复检索" />
        <div className="field">
          <span>清除包缓存</span>
          <small>已缓存的包将跳过在线检索直接使用历史结果；清除后所有包都会重新在线检索</small>
          <div style={{ display: 'flex', gap: '8px', alignItems: 'center', marginTop: '9px' }}>
            <button className="button ghost" onClick={onClearCache} style={{ marginLeft: 0 }}>清除缓存</button>
            <button className="button ghost" onClick={onExportDiagnostics}>导出诊断</button>
          </div>
        </div>
      </section>

      <section className="panel settings-panel">
        <PanelHeader step="镜像" title="CRAN 镜像" meta="实时影响脚本" />
        <div className="mirror-list">
          {mirrors.map((mirror) => (
            <button
              key={mirror.value}
              className={settings.cranMirror === mirror.value ? "selected" : ""}
              aria-pressed={settings.cranMirror === mirror.value}
              onClick={() => onMirrorSelect(mirror.value)}
            >
              <span>{mirror.label}</span>
              <code>{mirror.value}</code>
            </button>
          ))}
        </div>
        <label className="field compact">
          <span>自定义镜像</span>
          <input
            value={settings.cranMirror}
            onChange={(event) => onCranMirrorChange(event.currentTarget.value)}
            placeholder="https://cloud.r-project.org"
            maxLength={MAX_RESULT_FIELD_CHARS}
          />
        </label>
        <button className="button primary save-button" onClick={onSaveSettings} disabled={settingsBusy}>
          {settingsBusy ? "处理中..." : "保存设置"}
        </button>
      </section>
    </div>
  );
}
