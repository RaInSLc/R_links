import type { UpdateFailureStage, UpdateState, UpdaterConfigInfo } from "./types";
import { UPDATE_FAILURE_STAGE_LABEL, UPDATE_STATE_LABEL, resolveAppVersion } from "./utils-update";

interface Props {
  appVersion: string;
  updateState: UpdateState;
  updateStage: UpdateFailureStage | null;
  updaterConfig: UpdaterConfigInfo | null;
  onOpenUpdateSettings: () => void;
}

/**
 * 侧边栏底部的版本徽标：把「当前是什么版本」常驻在左下角，
 * 并用一个状态点反映上次检查更新的结果，点击直达更新设置。
 */
export function SidebarVersion({
  appVersion,
  updateState,
  updateStage,
  updaterConfig,
  onOpenUpdateSettings,
}: Props) {
  const version = resolveAppVersion(appVersion);
  const stateLabel = UPDATE_STATE_LABEL[updateState];
  const detail = updateStage ? `${stateLabel} · ${UPDATE_FAILURE_STAGE_LABEL[updateStage]}` : stateLabel;
  const endpoint = updaterConfig?.endpoints[0];
  const title = endpoint
    ? `当前版本 v${version}；更新状态：${detail}；更新源：${endpoint}`
    : `当前版本 v${version}；更新状态：${detail}`;
  return (
    <div className="sidebar-version">
      <button
        type="button"
        className="version-badge"
        onClick={onOpenUpdateSettings}
        title={title}
        aria-label={`当前版本 v${version}，点击查看更新设置`}
      >
        <span className={`version-dot ${updateState}`} aria-hidden="true" />
        <span className="version-label">版本</span>
        <strong className="version-number">v{version}</strong>
      </button>
      <small className={`version-state ${updateState}`}>{detail}</small>
    </div>
  );
}
