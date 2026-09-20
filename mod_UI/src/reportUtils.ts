import type { SearchResult } from "./utils";
import { generateMultiEcosystemScript } from "./utils-script";
import { defaultSettings, type Settings } from "./types";

export function mergeInstallCommands(results: SearchResult[], settings: Settings = defaultSettings): string {
  const found = results.filter((result) => result.found);
  const groups = new Map<string, SearchResult[]>();
  const separate: string[] = [];
  for (const result of found) {
    // 显式版本和归档地址必须保持单包生成器的精确语义。
    if (!result.requestedVersion && (result.source === "cran" && !result.repository.includes("/Archive/") && result.repository !== "archive" || result.source === "bioc")) {
      const group = groups.get(result.source) || [];
      if (!group.some((item) => item.package === result.package)) group.push(result);
      groups.set(result.source, group);
    } else {
      const command = getInstallCommand(result);
      if (!command) throw new Error("安装命令尚未生成，请稍后重试");
      separate.push(command);
    }
  }
  const quote = (value: string) => JSON.stringify(value);
  const merged = [...groups].map(([source, entries]) => {
    if (entries.some((entry) => !/^[A-Za-z][A-Za-z0-9.]*$/.test(entry.package))) throw new Error("包名不适合批量安装");
    const names = `c(${entries.map((entry) => quote(entry.package)).join(", ")})`;
    const packages = settings.conditional ? `Filter(function(p) !requireNamespace(p, quietly = TRUE), ${names})` : names;
    const lib = settings.rLibPath.trim() ? `, lib = ${quote(settings.rLibPath.trim())}` : "";
    const args = `dependencies = ${settings.installDependencies ? "TRUE" : "FALSE"}${lib}`;
    return source === "cran"
      ? `install.packages(${packages}, repos = ${quote(settings.cranMirror || defaultSettings.cranMirror)}, ${args})`
      : `BiocManager::install(${packages}, update = FALSE, ask = FALSE, ${args})`;
  });
  return ["# 合并未指定版本的 CRAN/Bioconductor 包，安装镜像当前版本；显式版本与其他来源保留原指令。", ...merged, ...new Set(separate)].join("\n");
}

export const CACHE_FEEDBACK_SOURCES = new Set(["cran", "bioc", "biocGit", "github", "r-forge"]);

export function isPlainMissingResult(result: SearchResult) {
  return !result.found && !["timeout", "rateLimited", "error"].includes(result.status || "");
}

export function isErrorResult(result: SearchResult) {
  return !result.found && ["timeout", "rateLimited", "error"].includes(result.status || "");
}

export function uniquePackages(results: SearchResult[]) {
  const seen = new Set<string>();
  return results.filter((result) => {
    const key = result.source === "github" ? result.package : result.package.toLocaleLowerCase();
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  }).map((result) => result.package);
}

export function isCacheHitResult(result: SearchResult) { return result.stage === "cacheHit"; }

export function getInstallCommand(result: SearchResult): string {
  const generated = (result as SearchResult & { installCommand?: string }).installCommand;
  if (generated !== undefined) return generated;
  if (!result.found) return result.package;
  if (result.source === "pip" || result.source === "conda") {
    const version = result.requestedVersion || result.latestVersion;
    const requirement = `${result.package}${version ? /^[=!<>~]/.test(version) ? version : `==${version}` : ""}`;
    return generateMultiEcosystemScript(
      requirement,
      result.source,
      result.source === "pip" ? result.repository : "",
      result.source === "conda" ? [result.repository || "conda-forge"] : [],
    )
      .split("\n")
      .filter((line) => line && !line.startsWith("#") && line !== "set -e")
      .join("\n");
  }
  if (result.source === "cran" && /\/Archive\//.test(result.repository)) return `remotes::install_url(${JSON.stringify(result.repository)}, upgrade = "never")`;
  if (result.source === "cran") return result.requestedVersion ? `remotes::install_version("${result.package}", version = "${result.requestedVersion}", repos = "https://cloud.r-project.org", upgrade = "never")` : `install.packages("${result.package}")`;
  if (result.source === "bioc") return `BiocManager::install("${result.package}", update = FALSE, ask = FALSE)`;
  if (result.source === "biocGit") return `remotes::install_git("https://git.bioconductor.org/packages/${result.package}", ref = ${JSON.stringify(`RELEASE_${result.repository.replace(/\./g, "_")}`)}, upgrade = "never")`;
  if (result.source === "github" && result.repository) {
    const version = result.requestedVersion;
    return `remotes::install_github("${result.repository}${version ? `@${version.startsWith("v") ? version : `v${version}`}` : ""}", upgrade = "never")`;
  }
  if (result.source === "r-forge") return `install.packages("${result.package}", repos = "http://R-Forge.R-project.org")`;
  return `install.packages("${result.package}")`;
}

export function resultSelectionKey(result: SearchResult) {
  return `${result.package}\u0001${result.requestedVersion}\u0001${result.source}\u0001${result.repository}\u0001${result.realName}`;
}

export function sourceCredibility(source: string, stage?: string) {
  if (stage === "cacheHit") return "缓存验证";
  if (source === "cran" || source === "bioc") return "官方源";
  if (source === "github") return "仓库验证";
  if (source === "r-forge") return "社区源";
  if (source === "pip") return "Pip Index";
  if (source === "conda") return "Conda Channel";
  return "未验证";
}
