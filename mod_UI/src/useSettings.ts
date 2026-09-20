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

type SettingsBoolField = "fullSearch" | "conditional" | "installDependencies" | "showRemoteVersion" | "useCache" | "useFilter" | "resolveDependencies" | "includeLightDependencies";
type SettingsPersistOverrides = Partial<
  Pick<
    Settings,
    | SettingsBoolField
    | "searchConcurrency"
    | "archiveGithubMajorGap"
    | "maxCacheEntries"
    | "maxDependencyDepth"
    | "maxDependencyNodes"
    | "proxy"
    | "githubToken"
    | "cranMirror"
    | "rLibPath"
    | "pinnedMethods"
  >
>;

type SetStatus = (s: string) => void;

export function useSettings(setStatus: SetStatus) {
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [showToken, setShowToken] = useState(false);
  const [tokenConfigured, setTokenConfigured] = useState(false);
  const [settingsBusy, setSettingsBusy] = useState(false);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const latestSettingsRef = useRef(defaultSettings);
  const settingsActionSeq = useRef(0);
  const settingsBusyRef = useRef(false);
  const pendingSettingsSaveRef = useRef(false);
  const userOverridesRef = useRef<Partial<Settings>>({});
  const settingsSavedRef = useRef(false);

  function applySettings(next: Settings) {
    latestSettingsRef.current = next;
    setSettings(next);
  }

  /** 收集用户显式改动的字段，供迟到的 load_settings 做增量合并。 */
  function collectChangedFields(previous: Settings, next: Settings): Partial<Settings> {
    const changed: Partial<Settings> = {};
    (Object.keys(previous) as Array<keyof Settings>).forEach((key) => {
      if (next[key] !== previous[key]) Object.assign(changed, { [key]: next[key] });
    });
    return changed;
  }

  useEffect(() => {
    let active = true;
    invoke<PublicSettings>("load_settings")
      .then((saved) => {
        // 若已经成功保存过一次，这份挂载时的快照就是陈旧数据，直接忽略。
        if (!active || settingsSavedRef.current) return;
        const clean = sanitizePublicSettings(saved);
        // 不再因为"加载期间发生过用户改动"就整份丢弃磁盘配置：只有用户显式改过的
        // 字段覆盖磁盘值，其余字段采用磁盘值，避免把用户其余已保存设置静默重置为默认值。
        applySettings({
          proxy: clean.proxy,
          githubToken: "",
           cranMirror: clean.cranMirror,
           rLibPath: clean.rLibPath,
          fullSearch: clean.fullSearch,
          searchConcurrency: clean.searchConcurrency,
          archiveGithubMajorGap: clean.archiveGithubMajorGap,
          conditional: clean.conditional,
          installDependencies: clean.installDependencies,
          showRemoteVersion: clean.showRemoteVersion,
          useCache: clean.useCache,
          maxCacheEntries: clean.maxCacheEntries,
          useFilter: clean.useFilter,
          resolveDependencies: clean.resolveDependencies,
          maxDependencyDepth: clean.maxDependencyDepth,
          includeLightDependencies: clean.includeLightDependencies,
          maxDependencyNodes: clean.maxDependencyNodes,
          pinnedMethods: clean.pinnedMethods,
          pipIndex: clean.pipIndex,
          condaChannels: clean.condaChannels,
          ...userOverridesRef.current,
        });
        setTokenConfigured(clean.githubTokenConfigured);
        setSettingsLoaded(true);
      })
      .catch((error) => {
        if (active) {
          setStatus(`设置加载失败: ${formatError(error)}`);
        }
      });
    return () => { active = false; };
  // 设置仅在挂载时加载一次：若把 setStatus 放进依赖，宿主传入非稳定回调时
  // 本 effect 会在每次渲染后重新发起 load_settings，形成「加载→应用→重渲染」死循环。
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function updateSettingsFromUser(update: (current: Settings) => Settings) {
    settingsActionSeq.current += 1;
    const previous = latestSettingsRef.current;
    const next = update(previous);
    userOverridesRef.current = {
      ...userOverridesRef.current,
      ...collectChangedFields(previous, next),
    };
    applySettings(next);
  }

  function replaceSettingsFromUser(next: Settings) {
    settingsActionSeq.current += 1;
    // 整份替换（如恢复默认值）：全部字段都视为用户意图，不再回退到磁盘值。
    userOverridesRef.current = { ...next };
    applySettings(next);
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
    field: keyof Pick<Settings, "proxy" | "githubToken" | "cranMirror" | "rLibPath">,
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

  async function persistSettings(overrides?: SettingsPersistOverrides) {
    if (settingsBusyRef.current) {
      pendingSettingsSaveRef.current = true;
      return;
    }
    if (!beginSettingsOperation()) return;
    const actionSeq = settingsActionSeq.current + 1;
    settingsActionSeq.current = actionSeq;
    const settingsSnapshot = overrides ? { ...latestSettingsRef.current, ...overrides } : latestSettingsRef.current;
    try {
      const publicSettings = sanitizePublicSettings(
        await invoke<PublicSettings>("save_settings", { settings: settingsSnapshot }),
      );
      setTokenConfigured(publicSettings.githubTokenConfigured);
      // 保存成功后，挂载时发出的 load_settings 快照即成为陈旧数据。
      settingsSavedRef.current = true;
      if (actionSeq !== settingsActionSeq.current) {
        setStatus("设置已保存；检测到新的界面修改，请再次保存");
        return;
      }
      applySettings({
        proxy: publicSettings.proxy,
        githubToken: "",
         cranMirror: publicSettings.cranMirror,
         rLibPath: publicSettings.rLibPath,
        fullSearch: publicSettings.fullSearch,
        searchConcurrency: publicSettings.searchConcurrency,
        archiveGithubMajorGap: publicSettings.archiveGithubMajorGap,
        conditional: publicSettings.conditional,
        installDependencies: publicSettings.installDependencies,
        showRemoteVersion: publicSettings.showRemoteVersion,
        useCache: publicSettings.useCache,
        maxCacheEntries: publicSettings.maxCacheEntries,
        useFilter: publicSettings.useFilter,
        resolveDependencies: publicSettings.resolveDependencies,
        maxDependencyDepth: publicSettings.maxDependencyDepth,
        includeLightDependencies: publicSettings.includeLightDependencies,
        maxDependencyNodes: publicSettings.maxDependencyNodes,
        pinnedMethods: publicSettings.pinnedMethods,
        pipIndex: publicSettings.pipIndex,
        condaChannels: publicSettings.condaChannels,
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
      const shouldFlush = pendingSettingsSaveRef.current;
      pendingSettingsSaveRef.current = false;
      endSettingsOperation();
      if (shouldFlush) void persistSettings();
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
      applySettings({
        ...latestSettingsRef.current,
        proxy: publicSettings.proxy,
        githubToken: "",
         cranMirror: publicSettings.cranMirror,
         rLibPath: publicSettings.rLibPath,
        fullSearch: publicSettings.fullSearch,
        searchConcurrency: publicSettings.searchConcurrency,
        archiveGithubMajorGap: publicSettings.archiveGithubMajorGap,
        conditional: publicSettings.conditional,
        installDependencies: publicSettings.installDependencies,
        showRemoteVersion: publicSettings.showRemoteVersion,
        useCache: publicSettings.useCache,
        maxCacheEntries: publicSettings.maxCacheEntries,
        useFilter: publicSettings.useFilter,
        resolveDependencies: publicSettings.resolveDependencies,
        maxDependencyDepth: publicSettings.maxDependencyDepth,
        includeLightDependencies: publicSettings.includeLightDependencies,
        maxDependencyNodes: publicSettings.maxDependencyNodes,
        pinnedMethods: publicSettings.pinnedMethods,
        pipIndex: publicSettings.pipIndex,
        condaChannels: publicSettings.condaChannels,
      });
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
    tokenConfigured, settingsBusy, settingsLoaded,
    updateSettingsFromUser,
    replaceSettingsFromUser,
    acceptSettingValue,
    persistSettings,
    clearSavedToken,
  };
}
