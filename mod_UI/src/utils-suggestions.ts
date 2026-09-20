import type { SearchResult, SmartSuggestion } from "./utils-types";
import { isActiveInputLine } from "./utils-sanitize";
import { dedupePackageInput, extractCanonicalInput } from "./utils-input";

const COMMON = [
  "ggplot2", "dplyr", "tidyr", "readr", "stringr", "purrr", "tibble", "rlang",
  "Seurat", "DESeq2", "edgeR", "limma", "BiocManager", "Rcpp", "data.table",
  "shiny", "rmarkdown", "knitr", "xml2", "sf", "terra",
];

function distance(a: string, b: string) {
  const row = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    let diagonal = row[0];
    row[0] = i;
    for (let j = 1; j <= b.length; j++) {
      const top = row[j];
      row[j] = a[i - 1] === b[j - 1]
        ? diagonal
        : Math.min(row[j] + 1, row[j - 1] + 1, diagonal + 1);
      diagonal = top;
    }
  }
  return row[b.length];
}

export function buildInputSmartSuggestions(
  input: string,
  profile: { total: number; archiveUrls: number; repositories: number },
  method: string,
  options: { verifyInstall?: boolean } = {},
): SmartSuggestion[] {
  if (!profile.total) return [];
  const out: SmartSuggestion[] = [];
  const lines = input.split(/\r?\n/).filter(isActiveInputLine);
  const hasCall = lines.some((l) =>
    /\b(?:install\.packages|BiocManager::install|library|require|remotes::install_|devtools::install_)\s*\(/.test(l),
  );
  const hasBioc = lines.some((l) =>
    /\b(?:BiocManager::install|bioconductor|bioc)\b/i.test(l),
  );
  const hasVersion = lines.some((l) =>
    /\b\d+\.\d+(?:[.\-][0-9A-Za-z]+)*\b/.test(l),
  );
  const canonical = hasCall ? extractCanonicalInput(input) : "";
  const items = lines.flatMap((l) => l.split(/[,;]/)).map((l) =>
    l.trim().toLowerCase(),
  );
  const typo = lines.length === 1 && /^[A-Za-z0-9._-]+$/.test(lines[0])
    ? COMMON.map((correct) => ({
      correct,
      d: distance(lines[0].toLowerCase(), correct.toLowerCase()),
    })).sort((a, b) => a.d - b.d)[0]
    : null;
  if (typo && typo.d > 0 && typo.d <= 2) {
    out.push({
      id: "package-typo",
      title: `您是否指的是 ${typo.correct}？`,
      detail: `检测到包名 ${lines[0]} 与常用包 ${typo.correct} 相近。`,
      actionLabel: "替换包名",
      action: "replaceInput",
      value: typo.correct,
    });
  }
  if (profile.archiveUrls === profile.total && !["remotes", "devtools"].includes(method)) {
    out.push({
      id: "archive-url",
      title: "检测到归档包 URL",
      detail: "建议使用 remotes 安装 URL，避免按普通 CRAN 包名生成脚本。",
      actionLabel: "切换到 remotes",
      action: "method",
      method: "remotes",
    });
  }
  if (profile.repositories === profile.total && method !== "github") {
    out.push({
      id: "github-repo",
      title: "检测到 GitHub 仓库格式",
      detail: "输入包含 owner/repo，建议直接使用 GitHub 安装方式。",
      actionLabel: "切换到 GitHub",
      action: "method",
      method: "github",
    });
  }
  if (hasBioc && method !== "biocManager" && !profile.archiveUrls && !profile.repositories) {
    out.push({
      id: "bioc-hint",
      title: "检测到 Bioconductor 线索",
      detail: "输入中包含 Bioconductor 或 BiocManager 语义，建议优先使用 Bioconductor 安装方式。",
      actionLabel: "切换到 Bioconductor",
      action: "method",
      method: "biocManager",
    });
  }
  if (hasVersion && method === "auto" && !profile.archiveUrls && !profile.repositories) {
    out.push({
      id: "version-hint",
      title: "检测到版本号",
      detail: "脚本会优先生成指定版本安装；检索后可同步确认远程版本。",
    });
  }
  if (hasCall) {
    out.push({
      id: "mixed-text",
      title: "检测到混合文本",
      detail: "输入中可能包含安装命令或日志，可先提取规范输入。",
      actionLabel: canonical ? "提取规范输入" : undefined,
      action: canonical ? "replaceInput" : undefined,
      value: canonical || undefined,
    });
  }
  if (new Set(items).size !== items.length) {
    out.push({
      id: "duplicate-packages",
      title: "检测到重复包名",
      detail: "输入中包含重复的包名，建议去重后再生成脚本。",
      actionLabel: "一键去重",
      action: "replaceInput",
      value: dedupePackageInput(input),
    });
  }
  if (profile.total > 20 && !options.verifyInstall) {
    out.push({
      id: "large-batch",
      title: "检测到批量安装任务",
      detail: "建议先检索来源并保留安装后验证，便于发现失败包。",
      actionLabel: "开启安装后验证",
      action: "enableVerify",
    });
  }
  return out.slice(0, 3);
}

export function buildResultSmartSuggestions(
  results: SearchResult[],
  options: { fullSearch?: boolean; searching?: boolean } = {},
): SmartSuggestion[] {
  if (options.searching || !results.length) return [];
  const out: SmartSuggestion[] = [];
  const limited = results.some((r) => r.status === "rateLimited");
  const errors = results.some((r) => r.status === "error" || r.status === "timeout");
  const missing = results.filter((r) => !r.found).length;
  const found = results.length - missing;
  if (limited) {
    out.push({
      id: "github-rate-limit",
      title: "检测到 GitHub 限流",
      detail: "建议在网络设置中配置 GitHub Token，提高检索配额并减少失败。",
      actionLabel: "打开网络设置",
      action: "openSettings",
    });
  }
  if (errors) {
    out.push({
      id: "search-errors",
      title: "检索存在超时或异常",
      detail: "建议先重试当前检索；如果仍失败，再检查代理、镜像或网络设置。",
      actionLabel: "重试检索",
      action: "retrySearch",
    });
  }
  if (missing && !found && !options.fullSearch) {
    out.push({
      id: "not-found-full-search",
      title: "未找到可用来源",
      detail: "建议启用全量检索，命中 CRAN 或 Bioconductor 后继续查询 GitHub。",
      actionLabel: "启用全量检索",
      action: "enableFullSearch",
    });
  }
  return out.slice(0, 3);
}
