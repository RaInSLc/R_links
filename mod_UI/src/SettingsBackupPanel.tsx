import { useRef } from "react";
import { PanelHeader } from "./components";
import { defaultInputRules, defaultSettings, type InputRules, type Settings } from "./types";
import { sanitizeImportedInputRules, sanitizeImportedSettings } from "./settingsSanitize";
interface Props {
  settings: Settings;
  inputRules: InputRules;
  onReplaceSettings: (value: Settings) => void;
  onSaveSettings: () => void;
  onThemeChange: (value: string) => void;
  onFontChange: (value: string) => void;
  onFontSizeChange: (value: number) => void;
  onReplaceInputRules: (value: InputRules) => void;
}
export function SettingsBackupPanel({
  settings,
  inputRules,
  onReplaceSettings,
  onSaveSettings,
  onThemeChange,
  onFontChange,
  onFontSizeChange,
  onReplaceInputRules,
}: Props) {
  const fileRef = useRef<HTMLInputElement>(null);
  const download = (name: string, content: string) => { const url = URL.createObjectURL(new Blob([content], { type: "application/json" })); const anchor = document.createElement("a"); anchor.href = url; anchor.download = name; document.body.appendChild(anchor); anchor.click(); document.body.removeChild(anchor); URL.revokeObjectURL(url); };
  const exportConfig = () => download(`rlinks_config_${new Date().toISOString().slice(0, 10)}.json`, JSON.stringify({ exportedAt: new Date().toISOString(), settings, inputRules, theme: localStorage.getItem("theme") || "office", fontFamily: localStorage.getItem("fontFamily") || "modern", fontSize: localStorage.getItem("fontSize") || "14" }, null, 2));
  const restore = () => { if (!window.confirm("确定恢复全部设置为默认值？此操作不可撤销。")) return; onReplaceSettings({ ...defaultSettings, githubToken: settings.githubToken }); onSaveSettings(); onThemeChange("office"); onFontChange("system"); onFontSizeChange(14); onReplaceInputRules({ ...defaultInputRules }); };
  const importConfig = async (event: React.ChangeEvent<HTMLInputElement>) => { const file = event.target.files?.[0]; event.target.value = ""; if (!file) return; try { const config = JSON.parse(await file.text()); if (config.settings) onReplaceSettings(sanitizeImportedSettings(config.settings, settings)); if (["office", "green", "graphite"].includes(config.theme)) onThemeChange(config.theme); if (["modern", "system", "classic"].includes(config.fontFamily)) onFontChange(config.fontFamily); const size = Number(config.fontSize); if (Number.isFinite(size) && size >= 12 && size <= 20) onFontSizeChange(size); if (config.inputRules) onReplaceInputRules(sanitizeImportedInputRules(config.inputRules, inputRules)); } catch { /* ignore invalid backup */ } };
  return <section className="panel settings-panel"><PanelHeader step="备份" title="配置备份" meta="导出/导入" /><div className="field" style={{ margin: "0 17px" }}><span>导出当前配置</span><small>将所有设置（策略、缓存、主题、字号、过滤规则等）导出为 JSON 文件，方便备份或迁移</small><div style={{ display: "flex", gap: "8px", marginTop: "9px" }}><button className="button ghost" onClick={exportConfig}>导出配置</button><button className="button ghost" onClick={() => fileRef.current?.click()}>导入配置</button><button className="button ghost" onClick={restore}>恢复默认</button><input ref={fileRef} type="file" accept=".json" style={{ display: "none" }} onChange={(event) => void importConfig(event)} /></div></div></section>;
}
