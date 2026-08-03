import type { SearchResult } from "./utils";

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
    const key = result.package.toLocaleLowerCase();
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  }).map((result) => result.package);
}

export function isCacheHitResult(result: SearchResult) { return result.stage === "cacheHit"; }

export function getInstallCommand(result: SearchResult): string {
  if (!result.found) return result.package;
  if (result.source === "pip") return `pip install ${result.package}${result.requestedVersion || result.latestVersion ? `==${result.requestedVersion || result.latestVersion}` : ""}`;
  if (result.source === "conda") return `conda install ${result.repository || "conda-forge"}::${result.package}${result.requestedVersion || result.latestVersion ? `=${result.requestedVersion || result.latestVersion}` : ""}`;
  if (result.source === "cran") return result.requestedVersion ? `remotes::install_version("${result.package}", version = "${result.requestedVersion}", repos = "https://cloud.r-project.org", upgrade = "never")` : `install.packages("${result.package}")`;
  if (result.source === "bioc") return `BiocManager::install("${result.package}", update = FALSE, ask = FALSE)`;
  if (result.source === "biocGit") return `remotes::install_git("https://git.bioconductor.org/packages/${result.package}", ref = "RELEASE_${(result.latestVersion || "3.18").split("|")[1]?.replace(".", "_") || "3_18"}", upgrade = "never")`;
  if (result.source === "github" && result.repository) {
    const version = result.requestedVersion || result.latestVersion;
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
