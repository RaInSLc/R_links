import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  formatError, githubTokenTextAllowed, settingsFieldLabel,
  settingsValueTooLargeOrUnsafe, sanitizePublicSettings,
  MAX_RESULT_FIELD_CHARS, MAX_TOKEN_CHARS,
  type PublicSettings,
} from "./utils";
import type { Settings } from "./types";
import { defaultSettings } from "./types";

type SettingsBoolField = "fullSearch" | "conditional" | "installDependencies" | "showRemoteVersion" | "useCache";

type SetStatus = (s: string) => void;

export function useSettings(setStatus: SetStatus) {
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [showToken, setShowToken] = useState(false);
  const [tokenConfigured, setTokenConfigured] = useState(false);
  const [settingsBusy, setSettingsBusy] = useState(false);
  const settingsActionSeq = useRef(0);
  const settingsBusyRef = useRef(false);

  useEffect(() => {
    let active = true;
    const loadSeq = settingsActionSeq.current;
    invoke<PublicSettings>("load_settings")
      .then((saved) => {
        if (!active || loadSeq !== settingsActionSeq.current) return;
        const clean = sanitizePublicSettings(saved);
        setSettings({
          proxy: clean.proxy,
          githubToken: "",
          cranMirror: clean.cranMirror,
          fullSearch: clean.fullSearch,
          conditional: clean.conditional,
          installDependencies: clean.installDependencies,
          showRemoteVersion: clean.showRemoteVersion,
          useCache: clean.useCache,
          maxCacheEntries: clean.maxCacheEntries,
        });
        setTokenConfigured(clean.githubTokenConfigured);
      })
      .catch((error) => {
        if (active && loadSeq === settingsActionSeq.current) {
          setStatus(`设置加载失败: ${formatError(error)}`);
        }
      });
    return () => { active = false; };
  }, []);

  function updateSettingsFromUser(update: (current: Settings) => Settings) {
    settingsActionSeq.current += 1;
    setSettings(update);
  }

  function beginSettingsOperation() {
    if (settingsBusyRef.current) {
      setStatus("设置操作正在进行，请稍候");
      return false;
    }
    settingsBusyRef.current = true;
    setSettingsBusy(true);
    return true;
  }

  function endSettingsOperation() {
    settingsBusyRef.current = false;
    setSettingsBusy(false);
  }

  function acceptSettingValue(
    field: keyof Pick<Settings, "proxy" | "githubToken" | "cranMirror">,
    value: string,
  ) {
    const nextValue = field === "proxy" ? value : value.trim();
    const label = settingsFieldLabel(field);
    const limit = field === "githubToken" ? MAX_TOKEN_CHARS : MAX_RESULT_FIELD_CHARS;
    if (settingsValueTooLargeOrUnsafe(nextValue, limit)) {
      setStatus(`${label}包含非法字符或长度过长，最多允许 ${limit} 字节`);
      return false;
    }
    if (field === "githubToken" && !githubTokenTextAllowed(nextValue)) {
      setStatus("GitHub Token 仅允许可见 ASCII 字符，不能包含空白字符");
      return false;
    }
    updateSettingsFromUser((current) => ({ ...current, [field]: nextValue }));
    return true;
  }

  async function persistSettings(overrides?: Partial<Pick<Settings, SettingsBoolField>>) {
    if (!beginSettingsOperation()) return;
    const actionSeq = settingsActionSeq.current + 1;
    settingsActionSeq.current = actionSeq;
    const settingsSnapshot = overrides ? { ...settings, ...overrides } : settings;
    try {
      const publicSettings = sanitizePublicSettings(
        await invoke<PublicSettings>("save_settings", { settings: settingsSnapshot }),
      );
      setTokenConfigured(publicSettings.githubTokenConfigured);
      if (actionSeq !== settingsActionSeq.current) {
        setStatus("设置已保存；检测到新的界面修改，请再次保存");
        return;
      }
      setSettings({
        proxy: publicSettings.proxy,
        githubToken: "",
        cranMirror: publicSettings.cranMirror,
        fullSearch: publicSettings.fullSearch,
        conditional: publicSettings.conditional,
        installDependencies: publicSettings.installDependencies,
        showRemoteVersion: publicSettings.showRemoteVersion,
        useCache: publicSettings.useCache,
        maxCacheEntries: publicSettings.maxCacheEntries,
      });
      setShowToken(false);
      setStatus("设置已保存并立即生效");
    } catch (error) {
      setStatus(
        actionSeq === settingsActionSeq.current
          ? `设置保存失败: ${formatError(error)}`
          : `先前设置保存失败，当前修改尚未保存: ${formatError(error)}`,
      );
    } finally {
      endSettingsOperation();
    }
  }

  async function clearSavedToken() {
    if (!beginSettingsOperation()) return;
    const actionSeq = settingsActionSeq.current + 1;
    settingsActionSeq.current = actionSeq;
    try {
      const publicSettings = sanitizePublicSettings(
        await invoke<PublicSettings>("clear_github_token"),
      );
      setTokenConfigured(false);
      if (actionSeq !== settingsActionSeq.current) {
        setStatus("已清除保存的 GitHub Token；界面保留了新的修改");
        return;
      }
      setSettings((current) => ({
        ...current,
        proxy: publicSettings.proxy,
        githubToken: "",
        cranMirror: publicSettings.cranMirror,
        fullSearch: publicSettings.fullSearch,
        conditional: publicSettings.conditional,
        installDependencies: publicSettings.installDependencies,
        showRemoteVersion: publicSettings.showRemoteVersion,
        useCache: publicSettings.useCache,
        maxCacheEntries: publicSettings.maxCacheEntries,
      }));
      setShowToken(false);
      setStatus("已清除保存的 GitHub Token");
    } catch (error) {
      setStatus(
        actionSeq === settingsActionSeq.current
          ? `Token 清除失败: ${formatError(error)}`
          : `Token 清除失败，当前修改未受影响: ${formatError(error)}`,
      );
    } finally {
      endSettingsOperation();
    }
  }

  return {
    settings, setSettings,
    showToken, setShowToken,
    tokenConfigured, settingsBusy,
    updateSettingsFromUser,
    acceptSettingValue,
    persistSettings,
    clearSavedToken,
  };
}