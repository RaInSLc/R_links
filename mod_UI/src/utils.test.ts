import { describe, it, expect } from "vitest";
import {
  safeText,
  safeBoolean,
  safeRunId,
  safeSource,
  safeStatusText,
  sanitizeStatus,
  sanitizeSearchStage,
  sanitizeSearchResult,
  sanitizeSearchResponse,
  formatError,
  truncateUtf8Bytes,
  utf8Length,
  isActiveInputLine,
  nonEmptyLineCountExceeds,
  dedupeBoundedResults,
  resultIdentityKey,
  buildInputSmartSuggestions,
  classifyInputProfile,
  buildSearchPlanPreview,
  buildResultSmartSuggestions,
  extractCanonicalInput,
  dedupePackageInput,
  normalizePackageInputDisplay,
  parseProjectDependencyFile,
  extractSystemRequirements,
  trimTrailingBlankLines,
  sanitizePublicSettings,
  countScriptCommands,
  countDuplicatePackages,
  MAX_STATUS_CHARS,
} from "./utils";

describe("safeText", () => {
  it("trims whitespace and strips control characters", () => {
    expect(safeText("  hello  ", 100)).toBe("hello");
    expect(safeText("a\tb\nc", 100)).toBe("abc");
  });

  it("truncates to byte limit", () => {
    expect(safeText("abcd", 2)).toBe("ab");
  });

  it("handles null/undefined", () => {
    expect(safeText(null, 100)).toBe("");
    expect(safeText(undefined, 100)).toBe("");
  });

  it("truncates multi-byte characters at byte boundary", () => {
    // Each Chinese char is 3 bytes in UTF-8
    const result = safeText("你好世界", 6);
    expect(result).toBe("你好");
  });
});

describe("normalizePackageInputDisplay", () => {
  it("preserves package manager commands and normalizes full-width separators", () => {
    expect(normalizePackageInputDisplay("pacman::p_load(dplyr，ggplot2)"))
      .toBe("pacman::p_load(dplyr，ggplot2)");
  });

  it("extracts packages from Rscript shell commands", () => {
    expect(extractCanonicalInput("RUN Rscript -e 'install.packages(c(\"dplyr\",\"tidyr\"))'"))
      .toBe("dplyr\ntidyr");
  });
});

describe("safeBoolean", () => {
  it("returns true only for literal true", () => {
    expect(safeBoolean(true)).toBe(true);
    expect(safeBoolean(false)).toBe(false);
    expect(safeBoolean(1)).toBe(false);
    expect(safeBoolean("true")).toBe(false);
    expect(safeBoolean(null)).toBe(false);
  });
});

describe("safeRunId", () => {
  it("accepts positive safe integers", () => {
    expect(safeRunId(1)).toBe(1);
    expect(safeRunId(999999)).toBe(999999);
  });

  it("rejects non-positive or non-integer", () => {
    expect(safeRunId(0)).toBe(0);
    expect(safeRunId(-1)).toBe(0);
    expect(safeRunId(1.5)).toBe(0);
    expect(safeRunId("1")).toBe(0);
    expect(safeRunId(null)).toBe(0);
  });
});

describe("safeSource", () => {
  it("accepts known sources", () => {
    expect(safeSource("cran")).toBe("cran");
    expect(safeSource("bioc")).toBe("bioc");
    expect(safeSource("github")).toBe("github");
    expect(safeSource("biocGit")).toBe("biocGit");
    expect(safeSource("none")).toBe("none");
  });

  it("returns 'none' for unknown sources", () => {
    expect(safeSource("npm")).toBe("none");
    expect(safeSource("")).toBe("none");
    expect(safeSource(null)).toBe("none");
  });
});

describe("safeStatusText", () => {
  it("strips control chars and truncates", () => {
    expect(safeStatusText("hello")).toBe("hello");
    expect(safeStatusText("  hello  ")).toBe("hello");
  });

  it("returns fallback for empty", () => {
    expect(safeStatusText("")).toBe("未知错误");
    expect(safeStatusText(null)).toBe("未知错误");
  });

  it("respects max length", () => {
    const long = "a".repeat(MAX_STATUS_CHARS + 100);
    const result = safeStatusText(long);
    expect(result.length).toBeLessThanOrEqual(MAX_STATUS_CHARS);
  });
});

describe("sanitizeStatus", () => {
  it("accepts valid status values", () => {
    expect(sanitizeStatus("found")).toBe("found");
    expect(sanitizeStatus("notFound")).toBe("notFound");
    expect(sanitizeStatus("timeout")).toBe("timeout");
    expect(sanitizeStatus("rateLimited")).toBe("rateLimited");
  });

  it("defaults to notFound for invalid values", () => {
    expect(sanitizeStatus("")).toBe("notFound");
    expect(sanitizeStatus("invalid")).toBe("notFound");
    expect(sanitizeStatus(null)).toBe("notFound");
    expect(sanitizeStatus(undefined)).toBe("notFound");
  });
});

describe("sanitizeSearchStage", () => {
  it("accepts valid stage values", () => {
    expect(sanitizeSearchStage("queued")).toBe("queued");
    expect(sanitizeSearchStage("cacheHit")).toBe("cacheHit");
    expect(sanitizeSearchStage("searching")).toBe("searching");
    expect(sanitizeSearchStage("retrying")).toBe("retrying");
    expect(sanitizeSearchStage("final")).toBe("final");
  });

  it("defaults to final for invalid values", () => {
    expect(sanitizeSearchStage("")).toBe("final");
    expect(sanitizeSearchStage("invalid")).toBe("final");
    expect(sanitizeSearchStage(null)).toBe("final");
    expect(sanitizeSearchStage(undefined)).toBe("final");
  });
});

describe("sanitizeSearchResult", () => {
  it("sanitizes a valid result", () => {
    const result = sanitizeSearchResult({
      package: "dplyr",
      requestedVersion: "",
      latestVersion: "1.1.0",
      repository: "",
      realName: "dplyr",
      source: "cran",
      found: true,
      message: "ok",
      status: "found",
      stage: "cacheHit",
    });
    expect(result.package).toBe("dplyr");
    expect(result.source).toBe("cran");
    expect(result.found).toBe(true);
    expect(result.status).toBe("found");
    expect(result.stage).toBe("cacheHit");
  });

  it("handles missing/null fields gracefully", () => {
    const result = sanitizeSearchResult({});
    expect(result.package).toBe("");
    expect(result.source).toBe("none");
    expect(result.found).toBe(false);
    expect(result.status).toBe("notFound");
    expect(result.stage).toBe("final");
  });

  it("rejects non-record input", () => {
    const result = sanitizeSearchResult("not an object");
    expect(result.package).toBe("");
    expect(result.source).toBe("none");
  });
});

describe("sanitizePublicSettings", () => {
  it("keeps explicit search concurrency within range", () => {
    const settings = sanitizePublicSettings({ searchConcurrency: 8, pinnedMethods: ["auto"] });
    expect(settings.searchConcurrency).toBe(8);
  });

  it("defaults invalid search concurrency", () => {
    expect(sanitizePublicSettings({ searchConcurrency: 0 }).searchConcurrency).toBe(6);
    expect(sanitizePublicSettings({ searchConcurrency: 99 }).searchConcurrency).toBe(6);
  });

  it("keeps explicit archive GitHub major gap within range", () => {
    const settings = sanitizePublicSettings({ archiveGithubMajorGap: 2, pinnedMethods: ["auto"] });
    expect(settings.archiveGithubMajorGap).toBe(2);
  });

  it("defaults invalid archive GitHub major gap", () => {
    expect(sanitizePublicSettings({ archiveGithubMajorGap: -1 }).archiveGithubMajorGap).toBe(1);
    expect(sanitizePublicSettings({ archiveGithubMajorGap: 99 }).archiveGithubMajorGap).toBe(1);
  });
});

describe("sanitizeSearchResponse", () => {
  it("sanitizes a full response", () => {
    const response = sanitizeSearchResponse({
      runId: 12345,
      results: [
        { package: "ggplot2", source: "cran", found: true, status: "found" },
        { package: "ggplot2", source: "cran", found: true, status: "found" },
      ],
      logs: ["line1", "line2"],
      stopped: false,
    });
    expect(response.runId).toBe(12345);
    expect(response.results).toHaveLength(1); // deduped
    expect(response.logs).toEqual(["line1", "line2"]);
    expect(response.stopped).toBe(false);
  });

  it("handles invalid runId", () => {
    const response = sanitizeSearchResponse({ runId: -1 });
    expect(response.runId).toBe(0);
  });

  it("handles non-object input", () => {
    const response = sanitizeSearchResponse("invalid");
    expect(response.runId).toBe(0);
    expect(response.results).toHaveLength(0);
    expect(response.logs).toHaveLength(0);
    expect(response.stopped).toBe(false);
  });

  it("sanitizes bounded stage timings and tolerates legacy responses", () => {
    const response = sanitizeSearchResponse({
      runId: 2,
      results: [],
      logs: [],
      stopped: false,
      stageTimings: [
        { stage: "多源检索", durationMs: 1200 },
        { stage: "恶意\u0000阶段", durationMs: -5 },
      ],
    });

    expect(response.stageTimings).toEqual([
      { stage: "多源检索", durationMs: 1200 },
      { stage: "恶意阶段", durationMs: 0 },
    ]);
    expect(sanitizeSearchResponse({ runId: 2, results: [], logs: [], stopped: false }).stageTimings).toEqual([]);
  });
});

describe("parseProjectDependencyFile", () => {
  it("extracts package versions from renv.lock", () => {
    expect(parseProjectDependencyFile("renv.lock", JSON.stringify({ Packages: { dplyr: { Package: "dplyr", Version: "1.1.4" }, rlang: { Package: "rlang" } } }))).toBe("dplyr 1.1.4\nrlang");
  });

  it("extracts R DESCRIPTION dependency fields", () => {
    expect(parseProjectDependencyFile("DESCRIPTION", "Package: demo\nImports: dplyr (>= 1.0),\n    rlang\nDepends: R (>= 4.0)\nLinkingTo: Rcpp")).toBe("dplyr\nrlang\nR\nRcpp");
  });

  it("preserves Python requirements constraints and ignores directives", () => {
    expect(parseProjectDependencyFile("requirements.txt", "numpy>=1.26\n# comment\n-r base.txt\npandas==2.2.0 # note")).toBe("numpy>=1.26\npandas==2.2.0");
  });
});

describe("extractSystemRequirements", () => {
  it("extracts multiline DESCRIPTION system requirements", () => {
    expect(extractSystemRequirements("DESCRIPTION", "Package: demo\nSystemRequirements: GDAL (>= 3.0),\n    PROJ\nImports: sf")).toBe("GDAL (>= 3.0), PROJ");
  });

  it("ignores system requirements in unrelated files", () => {
    expect(extractSystemRequirements("requirements.txt", "SystemRequirements: gcc")).toBeNull();
  });
});

describe("formatError", () => {
  it("formats Error messages", () => {
    expect(formatError(new Error("test error"))).toBe("test error");
  });

  it("formats string errors", () => {
    expect(formatError("string error")).toBe("string error");
  });

  it("returns fallback for unformattable", () => {
    const obj = { toString: () => { throw new Error("boom"); } };
    expect(formatError(obj)).toBe("未知错误");
  });
});

describe("truncateUtf8Bytes", () => {
  it("returns short strings unchanged", () => {
    expect(truncateUtf8Bytes("abc", 10)).toBe("abc");
  });

  it("truncates at byte boundary", () => {
    expect(truncateUtf8Bytes("abcdef", 3)).toBe("abc");
  });

  it("does not split multi-byte characters", () => {
    // 你 = 3 bytes, 好 = 3 bytes
    expect(truncateUtf8Bytes("你好", 4)).toBe("你");
    expect(truncateUtf8Bytes("你好", 3)).toBe("你");
    expect(truncateUtf8Bytes("你好", 2)).toBe("");
  });
});

describe("utf8Length", () => {
  it("counts ASCII correctly", () => {
    expect(utf8Length("hello")).toBe(5);
  });

  it("counts multi-byte characters", () => {
    expect(utf8Length("你好")).toBe(6);
  });
});

describe("isActiveInputLine", () => {
  it("returns true for non-empty non-comment lines", () => {
    expect(isActiveInputLine("dplyr")).toBe(true);
    expect(isActiveInputLine("  dplyr  ")).toBe(true);
  });

  it("returns false for empty or comment lines", () => {
    expect(isActiveInputLine("")).toBe(false);
    expect(isActiveInputLine("   ")).toBe(false);
    expect(isActiveInputLine("# comment")).toBe(false);
    expect(isActiveInputLine("  # indented comment")).toBe(false);
  });
});

describe("nonEmptyLineCountExceeds", () => {
  it("counts active lines and compares to limit", () => {
    expect(nonEmptyLineCountExceeds("a\nb\nc", 5)).toBe(false);
    expect(nonEmptyLineCountExceeds("a\nb\nc", 2)).toBe(true);
  });

  it("ignores comments and empty lines", () => {
    expect(nonEmptyLineCountExceeds("# comment\n\na", 5)).toBe(false);
    expect(nonEmptyLineCountExceeds("# comment\n\na\nb", 1)).toBe(true);
  });
});

describe("dedupeBoundedResults", () => {
  it("deduplicates by identity key", () => {
    const results = dedupeBoundedResults(
      [
        { package: "dplyr", source: "cran", found: true, status: "found" },
        { package: "dplyr", source: "cran", found: true, status: "found" },
      ],
      100,
      100,
    );
    expect(results).toHaveLength(1);
  });

  it("respects the limit", () => {
    const items = Array.from({ length: 10 }, (_, i) => ({
      package: `pkg${i}`,
      source: "cran",
      found: true,
    }));
    const results = dedupeBoundedResults(items, 3, 100);
    expect(results).toHaveLength(3);
  });
});

describe("resultIdentityKey", () => {
  it("produces consistent keys for same identity", () => {
    const a = {
      package: "DPLYR",
      source: "cran",
      repository: "",
      realName: "dplyr",
    } as never;
    const b = {
      package: "dplyr",
      source: "cran",
      repository: "",
      realName: "DPLYR",
    } as never;
    expect(resultIdentityKey(a)).toBe(resultIdentityKey(b));
  });
});

describe("buildSearchPlanPreview", () => {
  it("summarizes mixed input sources and estimates request scale", () => {
    const plan = buildSearchPlanPreview(
      { total: 10, archiveUrls: 1, repositories: 2 },
      { fullSearch: false, useCache: true },
    );

    expect(plan.cranLike).toBe(7);
    expect(plan.repositories).toBe(2);
    expect(plan.archiveUrls).toBe(1);
    expect(plan.estimatedRequests).toBe(12);
    expect(plan.level).toBe("light");
    expect(plan.summary).toContain("10 个输入");
    expect(plan.summary).toContain("GitHub 2");
  });

  it("marks large full searches as heavy and recommends safeguards", () => {
    const plan = buildSearchPlanPreview(
      { total: 90, archiveUrls: 0, repositories: 5 },
      { fullSearch: true, useCache: false },
    );

    expect(plan.level).toBe("heavy");
    expect(plan.recommendedMode).toBe("全量检索");
    expect(plan.advice).toContain("GitHub Token");
  });
});

describe("buildInputSmartSuggestions", () => {
  it("suggests GitHub method for repository input", () => {
    const suggestions = buildInputSmartSuggestions(
      "satijalab/seurat",
      { total: 1, archiveUrls: 0, repositories: 1 },
      "auto",
    );
    expect(suggestions[0]).toMatchObject({ id: "github-repo", method: "github" });
  });

  it("suggests remotes for archive URLs", () => {
    const suggestions = buildInputSmartSuggestions(
      "https://example.org/pkg_1.0.tar.gz",
      { total: 1, archiveUrls: 1, repositories: 0 },
      "auto",
    );
    expect(suggestions[0]).toMatchObject({ id: "archive-url", method: "remotes" });
  });

  it("detects version hints and mixed text", () => {
    const suggestions = buildInputSmartSuggestions(
      "install.packages(\"dplyr\")\nggplot2 3.5.0",
      { total: 2, archiveUrls: 0, repositories: 0 },
      "auto",
    );
    expect(suggestions.map((item) => item.id)).toEqual(["version-hint", "mixed-text"]);
    expect(suggestions[1]).toMatchObject({ action: "replaceInput", value: "dplyr" });
  });

  it("suggests extracting canonical input from R commands", () => {
    const suggestions = buildInputSmartSuggestions(
      "install.packages(c(\"dplyr\", \"ggplot2\"))\nlibrary('Seurat')",
      { total: 3, archiveUrls: 0, repositories: 0 },
      "auto",
    );
    expect(suggestions).toContainEqual(expect.objectContaining({
      id: "mixed-text",
      action: "replaceInput",
      value: "dplyr\nggplot2\nSeurat",
    }));
  });

  it("suggests Bioconductor method when input contains Bioc hints", () => {
    const suggestions = buildInputSmartSuggestions(
      "BiocManager::install(\"GSVA\")",
      { total: 1, archiveUrls: 0, repositories: 0 },
      "auto",
    );
    expect(suggestions[0]).toMatchObject({ id: "bioc-hint", method: "biocManager" });
  });

  it("suggests enabling verification for large batches", () => {
    const input = Array.from({ length: 21 }, (_, index) => `pkg${index}`).join("\n");
    const suggestions = buildInputSmartSuggestions(
      input,
      { total: 21, archiveUrls: 0, repositories: 0 },
      "auto",
      { verifyInstall: false },
    );
    expect(suggestions).toContainEqual(expect.objectContaining({ id: "large-batch", action: "enableVerify" }));
  });

  it("does not suggest enabling verification when it is already on", () => {
    const input = Array.from({ length: 21 }, (_, index) => `pkg${index}`).join("\n");
    const suggestions = buildInputSmartSuggestions(
      input,
      { total: 21, archiveUrls: 0, repositories: 0 },
      "auto",
      { verifyInstall: true },
    );
    expect(suggestions.map((item) => item.id)).not.toContain("large-batch");
  });
});

describe("extractCanonicalInput", () => {
  it("extracts unique packages and repositories from common R commands", () => {
    expect(extractCanonicalInput([
      "install.packages(c(\"dplyr\", \"ggplot2\"))",
      "BiocManager::install('GSVA')",
      "remotes::install_github(\"tidyverse/dplyr\")",
      "library(ggplot2)",
      "Warning: package not available",
    ].join("\n"))).toBe("dplyr\nggplot2\nGSVA\ntidyverse/dplyr");
  });
});

describe("dedupePackageInput", () => {
  it("removes duplicate package names case-insensitively", () => {
    expect(dedupePackageInput("dplyr\nggplot2\nDPLYR\nggplot2\ntidyr")).toBe("dplyr\nggplot2\ntidyr");
  });

  it("preserves comments and empty lines", () => {
    expect(dedupePackageInput("# comment\ndplyr\n\nggplot2\ndplyr")).toBe("# comment\ndplyr\n\nggplot2");
  });

  it("handles URL lines", () => {
    const url = "https://example.org/pkg_1.0.tar.gz";
    expect(dedupePackageInput(`${url}\n${url}`)).toBe(url);
  });

  it("handles local HTTP URL lines", () => {
    const url = "http://192.168.5.250:8011/pkg_1.0.tar.gz";
    expect(dedupePackageInput(`${url}\n${url}`)).toBe(url);
  });
});

describe("classifyInputProfile", () => {
  it("classifies HTTP archive URLs as URL inputs", () => {
    expect(classifyInputProfile(
      "http://192.168.5.250:8011/pkg_1.0.tar.gz\ndplyr",
    )).toEqual({ total: 2, archiveUrls: 1, repositories: 0 });
  });

  it("uses configured separators for input statistics", () => {
    expect(classifyInputProfile("dplyr|ggplot2", ["|"])).toEqual({
      total: 2,
      archiveUrls: 0,
      repositories: 0,
    });
  });
});

describe("search result identity", () => {
  it("keeps the same package at different requested versions separate", () => {
    const base = {
      package: "dplyr",
      requestedVersion: "1.0.0",
      latestVersion: "1.0.0",
      repository: "",
      realName: "dplyr",
      source: "cran",
      found: true,
      message: "ok",
    };
    const other = { ...base, requestedVersion: "2.0.0", latestVersion: "2.0.0" };
    expect(resultIdentityKey(base)).not.toBe(resultIdentityKey(other));
    expect(dedupeBoundedResults([base, other], 10, 10)).toHaveLength(2);
  });
});

describe("normalizePackageInputDisplay", () => {
  it("converts pasted Markdown package tables into one package per line", () => {
    expect(normalizePackageInputDisplay([
      "| 包名                | 来源        |",
      "| ----------------- | --------- |",
      "| DMwR              | CRAN（已归档） |",
      "| kernelshap        | CRAN      |",
      "| extraTrees        | CRAN      |",
    ].join("\n"))).toBe("DMwR\nkernelshap\nextraTrees");
  });

  it("removes trailing blank lines from pasted Markdown package tables", () => {
    expect(normalizePackageInputDisplay([
      "| 包名 | 来源 |",
      "| --- | --- |",
      "| DMwR | CRAN |",
      "| kernelshap | CRAN |",
      "",
      "",
    ].join("\n"))).toBe("DMwR\nkernelshap");
  });
});

describe("trimTrailingBlankLines", () => {
  it("removes trailing blank lines without touching middle blank lines", () => {
    expect(trimTrailingBlankLines("pkg1\n\npkg2\n\n\n")).toBe("pkg1\n\npkg2");
    expect(trimTrailingBlankLines("pkg1\n  \n\t")).toBe("pkg1");
  });
});

describe("buildInputSmartSuggestions duplicates", () => {
  it("suggests deduplication when input has duplicate packages", () => {
    const suggestions = buildInputSmartSuggestions(
      "dplyr\nggplot2\ndplyr",
      { total: 3, archiveUrls: 0, repositories: 0 },
      "auto",
    );
    expect(suggestions).toContainEqual(expect.objectContaining({
      id: "duplicate-packages",
      action: "replaceInput",
      value: "dplyr\nggplot2",
    }));
  });

  it("does not suggest deduplication when no duplicates exist", () => {
    const suggestions = buildInputSmartSuggestions(
      "dplyr\nggplot2\ntidyr",
      { total: 3, archiveUrls: 0, repositories: 0 },
      "auto",
    );
    expect(suggestions.map((s) => s.id)).not.toContain("duplicate-packages");
  });
});

describe("countScriptCommands", () => {
  it("counts install and library calls", () => {
    expect(countScriptCommands('install.packages("dplyr")\nlibrary(ggplot2)')).toBe(2);
  });

  it("ignores comments and blanks", () => {
    expect(countScriptCommands('# comment\n\nif (TRUE) {\n  install.packages("dplyr")\n}')).toBe(1);
  });

  it("returns 0 for placeholder", () => {
    expect(countScriptCommands("等待输入...")).toBe(0);
  });
});

describe("countDuplicatePackages", () => {
  it("counts duplicates case-insensitively", () => {
    expect(countDuplicatePackages("dplyr\nDPLYR\nggplot2")).toBe(1);
  });

  it("returns 0 when no duplicates", () => {
    expect(countDuplicatePackages("dplyr\nggplot2")).toBe(0);
  });
});

describe("buildResultSmartSuggestions", () => {
  it("suggests opening settings for GitHub rate limits", () => {
    const suggestions = buildResultSmartSuggestions([
      { package: "pkg", requestedVersion: "", latestVersion: "", repository: "", realName: "pkg", source: "github", found: false, message: "rate limit", status: "rateLimited" },
    ]);
    expect(suggestions[0]).toMatchObject({ id: "github-rate-limit", action: "openSettings" });
  });

  it("suggests full search when all packages are missing", () => {
    const suggestions = buildResultSmartSuggestions([
      { package: "pkg", requestedVersion: "", latestVersion: "", repository: "", realName: "pkg", source: "none", found: false, message: "missing", status: "notFound" },
    ], { fullSearch: false });
    expect(suggestions[0]).toMatchObject({ id: "not-found-full-search", action: "enableFullSearch" });
  });

  it("does not show result suggestions while searching", () => {
    const suggestions = buildResultSmartSuggestions([
      { package: "pkg", requestedVersion: "", latestVersion: "", repository: "", realName: "pkg", source: "none", found: false, message: "missing", status: "notFound" },
    ], { searching: true });
    expect(suggestions).toHaveLength(0);
  });

  it("suggests retrying when results contain timeouts or errors", () => {
    const suggestions = buildResultSmartSuggestions([
      { package: "pkg", requestedVersion: "", latestVersion: "", repository: "", realName: "pkg", source: "none", found: false, message: "timeout", status: "timeout" },
    ]);
    expect(suggestions[0]).toMatchObject({ id: "search-errors", action: "retrySearch" });
  });
});
