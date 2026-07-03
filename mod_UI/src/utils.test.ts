import { describe, it, expect } from "vitest";
import {
  safeText,
  safeBoolean,
  safeRunId,
  safeSource,
  safeStatusText,
  sanitizeStatus,
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
    });
    expect(result.package).toBe("dplyr");
    expect(result.source).toBe("cran");
    expect(result.found).toBe(true);
    expect(result.status).toBe("found");
  });

  it("handles missing/null fields gracefully", () => {
    const result = sanitizeSearchResult({});
    expect(result.package).toBe("");
    expect(result.source).toBe("none");
    expect(result.found).toBe(false);
    expect(result.status).toBe("notFound");
  });

  it("rejects non-record input", () => {
    const result = sanitizeSearchResult("not an object");
    expect(result.package).toBe("");
    expect(result.source).toBe("none");
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
