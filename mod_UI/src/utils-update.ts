import { formatError } from "./utils-sanitize";
import type { UpdateFailureStage, UpdateState, UpdaterConfigInfo } from "./types";

/**
 * 构建期由 `vite.config.ts` 注入的版本号（取自 package.json）。
 * 运行时 `getVersion()` 失败时用它兜底，避免界面出现空版本或写死的过期版本。
 */
export const BUILD_APP_VERSION = typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "";

export function resolveAppVersion(runtimeVersion: string): string {
  const trimmed = (runtimeVersion || "").trim();
  return trimmed || BUILD_APP_VERSION;
}

export const UPDATE_STATE_LABEL: Record<UpdateState, string> = {
  idle: "未检查",
  checking: "检查中",
  available: "发现更新",
  downloading: "下载中",
  installing: "安装中",
  readyToRestart: "待重启",
  upToDate: "已是最新",
  error: "检查失败",
};

export const UPDATE_FAILURE_STAGE_LABEL: Record<UpdateFailureStage, string> = {
  "manifest-missing": "更新清单缺失",
  signature: "签名校验失败",
  permission: "更新权限不足",
  network: "网络不可达",
  unknown: "未知错误",
};

/**
 * 判定顺序有讲究：清单缺失与签名失败都来自「请求成功但内容/校验不通过」，
 * 必须排在网络类之前，否则 `404` 这类响应会被误判成断网。
 */
const FAILURE_PATTERNS: ReadonlyArray<readonly [UpdateFailureStage, RegExp]> = [
  [
    "manifest-missing",
    /valid release json|404|not found|missing field|invalid json|expected value|unsupported version/i,
  ],
  ["signature", /signature|public key|pubkey|minisign|verify/i],
  ["permission", /not allowed|forbidden|denied|permission|capability|not permitted/i],
  [
    "network",
    /timeout|timed out|error sending request|network error|failed to fetch|dns|connect|proxy|tls|certificate/i,
  ],
];

export function classifyUpdateFailure(error: unknown): UpdateFailureStage {
  const message = formatError(error);
  for (const [stage, pattern] of FAILURE_PATTERNS) {
    if (pattern.test(message)) return stage;
  }
  return "unknown";
}

interface FailureContext {
  stage: UpdateFailureStage;
  rawMessage: string;
  config: UpdaterConfigInfo | null;
}

/**
 * 把失败阶段翻译成「下一步该做什么」。更新链路跨 GitHub Release、签名密钥、
 * capabilities 与本地网络四层，笼统的「检查更新失败」无法定位，因此这里必须
 * 带上真正生效的更新端点与公钥信息。
 */
export function describeUpdateFailure({ stage, rawMessage, config }: FailureContext): string {
  const endpoint = config?.endpoints[0] ?? "未配置更新端点";
  const keyId = config?.pubkeyKeyId;
  switch (stage) {
    case "manifest-missing":
      return `更新源缺少 latest.json 自动更新清单（${endpoint}）。发布该版本时未生成清单或密钥未配置，应用内无法自动更新，请到 GitHub Releases 手动下载安装包。`;
    case "signature":
      return keyId
        ? `更新包签名校验失败：发布所用签名密钥与应用内置公钥（${keyId}）不一致。请手动安装，并核对发布流程的签名密钥。`
        : "更新包签名校验失败：应用内置的签名公钥无法解析。请在 tauri.conf.json 的 plugins.updater.pubkey 填入合法公钥后重新打包发布。";
    case "permission":
      return "更新权限不足：capabilities/default.json 缺少 updater 权限声明。";
    case "network":
      return `无法连接更新源（${endpoint}）。请确认网络可访问 GitHub，或在网络设置中配置代理后重试。`;
    default:
      return `检查更新失败：${rawMessage}`;
  }
}

export function describeUpdateFailureWithRaw(context: FailureContext): string {
  const summary = describeUpdateFailure(context);
  return context.rawMessage && !summary.includes(context.rawMessage)
    ? `${summary}（原始错误：${context.rawMessage}）`
    : summary;
}
