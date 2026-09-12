import { describe, it, expect } from "vitest";
import {
  buildInputSmartSuggestions,
  buildSearchPlanPreview,
  buildResultSmartSuggestions,
} from "./utils";

describe("buildInputSmartSuggestions typo correction", () => {
  it("suggests ggplot2 for a close typo", () => {
    const suggestions = buildInputSmartSuggestions("gplot2", { total: 1, archiveUrls: 0, repositories: 0 }, "auto");
    expect(suggestions.find((suggestion) => suggestion.id === "package-typo")).toMatchObject({ value: "ggplot2", action: "replaceInput" });
  });

  it("does not suggest corrections for URLs or repositories", () => {
    const suggestions = buildInputSmartSuggestions("owner/gplot2", { total: 1, archiveUrls: 0, repositories: 1 }, "auto");
    expect(suggestions.some((suggestion) => suggestion.id === "package-typo")).toBe(false);
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
