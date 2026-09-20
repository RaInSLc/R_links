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
  dedupeBoundedResults,
  resultIdentityKey,
  sanitizePublicSettings,
  diagnoseDependencyGraph,
  MAX_STATUS_CHARS,
} from "./utils";
import type { DependencyNode } from "./utils";

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

  it("keeps GitHub packages distinct when only case differs", () => {
    const upper = { package: "Scissor", source: "github", repository: "sunduanchen/Scissor", realName: "Scissor" } as never;
    const lower = { package: "scissor", source: "github", repository: "statgarten/scissor", realName: "scissor" } as never;
    expect(resultIdentityKey(upper)).not.toBe(resultIdentityKey(lower));
    expect(dedupeBoundedResults([upper, lower], 10, 10)).toHaveLength(2);
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

describe("diagnoseDependencyGraph", () => {
  const node = (pkg: string, version: string): DependencyNode => ({ package: pkg, source: "cran", version, depth: 0, rootPackages: [pkg], directDependencyCount: 0, heavyDependencyCount: 0, status: "resolved" });
  it("detects cycles and version conflicts", () => {
    const diagnostics = diagnoseDependencyGraph({ roots: ["a"], nodes: [node("a", "1.0"), node("b", "1.0"), node("A", "2.0")], edges: [{ from: "a", to: "b", relation: "Imports", strength: "heavy", depth: 1 }, { from: "b", to: "a", relation: "Imports", strength: "heavy", depth: 1 }], summary: { totalNodes: 3, totalEdges: 2, heavyNodes: 2, lightNodes: 0, sharedNodes: 0 } });
    expect(diagnostics.map((item) => item.type)).toEqual(expect.arrayContaining(["cycle", "version-conflict"]));
  });

  it("多汇聚路径下仍能检测到环（记忆化不改变环检测结果）", () => {
    // a -> b -> d -> b 构成环，同时 a -> c -> d 与 b 共享汇聚点 d。
    // 记忆化剪枝后环仍必须被恰好报告一次。
    const graph = {
      roots: ["a"],
      nodes: [node("a", "1.0"), node("b", "1.0"), node("c", "1.0"), node("d", "1.0")],
      edges: [
        { from: "a", to: "b", relation: "Imports", strength: "heavy", depth: 1 },
        { from: "a", to: "c", relation: "Imports", strength: "heavy", depth: 1 },
        { from: "b", to: "d", relation: "Imports", strength: "heavy", depth: 2 },
        { from: "c", to: "d", relation: "Imports", strength: "heavy", depth: 2 },
        { from: "d", to: "b", relation: "Imports", strength: "heavy", depth: 3 },
      ],
      summary: { totalNodes: 4, totalEdges: 5, heavyNodes: 4, lightNodes: 0, sharedNodes: 1 },
    };
    const cycles = diagnoseDependencyGraph(graph).filter((item) => item.type === "cycle");
    expect(cycles).toHaveLength(1);
    expect(cycles[0].packages).toEqual(["b", "d", "b"]);
  });

  it("大型汇聚图上不会重复展开同一节点（记忆化生效）", () => {
    // 25 层共享汇聚点：无记忆化时路径数随层数指数增长，此用例会直接卡死。
    const layers = 25;
    const nodes = [node("root", "1.0")];
    const edges = [{ from: "root", to: "l0a", relation: "Imports", strength: "heavy", depth: 1 }, { from: "root", to: "l0b", relation: "Imports", strength: "heavy", depth: 1 }];
    for (let i = 0; i < layers; i += 1) {
      nodes.push(node(`l${i}a`, "1.0"), node(`l${i}b`, "1.0"));
      const next = i + 1 < layers ? [`l${i + 1}a`, `l${i + 1}b`] : [];
      for (const from of [`l${i}a`, `l${i}b`]) {
        for (const to of next) edges.push({ from, to, relation: "Imports", strength: "heavy", depth: i + 2 });
      }
    }
    const started = Date.now();
    const diagnostics = diagnoseDependencyGraph({
      roots: ["root"], nodes, edges,
      summary: { totalNodes: nodes.length, totalEdges: edges.length, heavyNodes: nodes.length, lightNodes: 0, sharedNodes: 0 },
    });
    expect(diagnostics.filter((item) => item.type === "cycle")).toHaveLength(0);
    expect(Date.now() - started).toBeLessThan(2000);
  });
});
