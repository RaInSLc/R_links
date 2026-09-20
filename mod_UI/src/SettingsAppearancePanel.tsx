import { PanelHeader } from "./components";
import type { UpdateFailureStage, UpdateState, UpdaterConfigInfo } from "./types";
import { UPDATE_FAILURE_STAGE_LABEL, UPDATE_STATE_LABEL, resolveAppVersion } from "./utils-update";

interface Props {
  currentTheme: string;
  currentFont: string;
  currentFontSize: number;
  checkingUpdate: boolean;
  updateState: UpdateState;
  updateStage: UpdateFailureStage | null;
  updateMessage: string;
  appVersion: string;
  updateVersion: string;
  updaterConfig: UpdaterConfigInfo | null;
  onThemeChange: (value: string) => void;
  onFontChange: (value: string) => void;
  onFontSizeChange: (value: number) => void;
  onCheckUpdates: () => void;
}

const themes = ["office", "green", "graphite"] as const;
const fonts = ["modern", "system", "classic"] as const;

const THEME_LABELS: Record<string, string> = {
  office: "商务办公蓝",
  green: "墨绿林野",
  graphite: "石墨暗灰",
};
const THEME_DOTS: Record<string, string[]> = {
  office: ["#0f172a", "#0f4c81", "#e6f0fa"],
  green: ["#112c24", "#176b4d", "#dcece4"],
  graphite: ["#212529", "#495057", "#f1f3f5"],
};
const FONT_LABELS: Record<string, string> = {
  modern: "现代 (推荐)",
  system: "系统默认",
  classic: "传统宋体",
};
const FONT_STACKS: Record<string, string> = {
  modern: "'Inter', 'Noto Sans SC', sans-serif",
  system: '"Segoe UI", "Microsoft YaHei UI", sans-serif',
  classic: '"SimSun", "宋体", serif',
};

export function SettingsAppearancePanel({
  currentTheme,
  currentFont,
  currentFontSize,
  checkingUpdate,
  updateState,
  updateStage,
  updateMessage,
  appVersion,
  updateVersion,
  updaterConfig,
  onThemeChange,
  onFontChange,
  onFontSizeChange,
  onCheckUpdates,
}: Props) {
  const version = resolveAppVersion(appVersion);
  const endpoint = updaterConfig?.endpoints[0] ?? "未配置";
  const keyId = updaterConfig?.pubkeyKeyId ?? "无法解析";
  const stageSuffix = updateStage ? `；失败阶段：${UPDATE_FAILURE_STAGE_LABEL[updateStage]}` : "";
  return (
    <>
      <section className="panel settings-panel">
        <PanelHeader step="界面" title="显示偏好" meta="主题与字号" />
        <div className="field" style={{ margin: "0 17px" }}>
          <span>界面风格</span>
          <small>选择您偏好的系统色彩，切换实时生效</small>
          <div className="theme-selector">
            {themes.map((theme) => (
              <button
                key={theme}
                type="button"
                className={`theme-card ${currentTheme === theme ? "selected" : ""}`}
                onClick={() => onThemeChange(theme)}
              >
                <div className="theme-preview-dots">
                  {THEME_DOTS[theme].map((color) => (
                    <div key={color} className="theme-dot" style={{ background: color }} />
                  ))}
                </div>
                <span>{THEME_LABELS[theme]}</span>
              </button>
            ))}
          </div>
        </div>
        <div className="field" style={{ margin: "0 17px", marginTop: "24px" }}>
          <span>字体风格</span>
          <small>选择最适合您显示器的排版</small>
          <div className="theme-selector">
            {fonts.map((font) => (
              <button
                key={font}
                type="button"
                className={`theme-card ${currentFont === font ? "selected" : ""}`}
                onClick={() => onFontChange(font)}
              >
                <div className="theme-preview-dots" style={{ alignItems: "center", justifyContent: "center" }}>
                  <span
                    style={{
                      fontFamily: FONT_STACKS[font],
                      fontSize: "15px",
                      fontWeight: 600,
                      color: "var(--ink)",
                    }}
                  >
                    Aa
                  </span>
                </div>
                <span>{FONT_LABELS[font]}</span>
              </button>
            ))}
          </div>
        </div>
        <div className="field" style={{ margin: "0 17px", marginTop: "24px" }}>
          <span>界面字号</span>
          <small>拖动滑块实时调整界面字体大小（{currentFontSize}px）</small>
          <div style={{ display: "flex", alignItems: "center", gap: "12px", marginTop: "8px" }}>
            <span style={{ fontSize: "12px", color: "var(--muted)" }}>A</span>
            <input
              type="range"
              min={12}
              max={20}
              step={1}
              value={currentFontSize}
              onChange={(event) => onFontSizeChange(Number(event.currentTarget.value))}
              style={{ flex: 1, accentColor: "var(--theme-color)" }}
            />
            <span style={{ fontSize: "20px", color: "var(--muted)" }}>A</span>
            <span style={{ fontSize: "13px", color: "var(--muted)", minWidth: "32px", textAlign: "right" }}>
              {currentFontSize}px
            </span>
          </div>
          <div className="font-preview-box" style={{ fontSize: `${currentFontSize}px` }}>
            安装包 Seurat · 版本 5.2.1 · 来源 CRAN — 字号预览
          </div>
        </div>
      </section>
      <section className="panel settings-panel">
        <PanelHeader step="系统" title="应用更新" meta="版本维护" />
        <div className="field">
          <span>当前版本 v{version}</span>
          <small>更新源：{endpoint}</small>
          <small>内置签名公钥：{keyId}</small>
          <div style={{ display: "flex", gap: "8px", alignItems: "center", marginTop: "9px", flexWrap: "wrap" }}>
            <button className="button primary" onClick={onCheckUpdates} disabled={checkingUpdate}>
              {checkingUpdate ? "正在处理..." : updateState === "error" ? "重试更新" : "检查更新"}
            </button>
            <span style={{ fontSize: "13px", color: "var(--muted)" }}>
              状态：{UPDATE_STATE_LABEL[updateState]}
              {stageSuffix}
            </span>
            {updateVersion && (
              <span style={{ fontSize: "13px", color: "var(--muted)" }}>目标版本：{updateVersion}</span>
            )}
          </div>
          {updateMessage && (
            <small
              style={{
                display: "block",
                marginTop: "8px",
                color: updateState === "error" ? "var(--red)" : "var(--muted)",
                lineHeight: 1.6,
              }}
            >
              {updateMessage}
            </small>
          )}
        </div>
      </section>
    </>
  );
}
