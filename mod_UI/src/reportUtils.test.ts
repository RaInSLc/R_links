import { describe, it, expect } from "vitest";
import { getInstallCommand } from "./reportUtils";
import type { SearchResult } from "./utils-types";

function result(overrides: Partial<SearchResult>): SearchResult {
  return {
    package: "demo",
    requestedVersion: "",
    latestVersion: "1.0.0",
    repository: "",
    realName: "demo",
    source: "cran",
    found: true,
    message: "",
    status: "found",
    stage: "final",
    ...overrides,
  };
}

describe("getInstallCommand 的 pip / conda 兜底", () => {
  it("pip 索引地址非法时返回空串而不是抛错", () => {
    // 本函数在 ReportActions / ReportResultsTable 的渲染期被调用，
    // 一旦抛出会让整张报告页被错误边界接管。
    const broken = result({ source: "pip", repository: "not-a-url" });
    expect(() => getInstallCommand(broken)).not.toThrow();
    expect(getInstallCommand(broken)).toBe("");
  });

  it("conda 渠道名非法时返回空串而不是抛错", () => {
    const broken = result({ source: "conda", repository: "evil channel" });
    expect(() => getInstallCommand(broken)).not.toThrow();
    expect(getInstallCommand(broken)).toBe("");
  });

  it("合法配置仍生成可复制的安装命令", () => {
    const pip = result({ source: "pip", repository: "https://pypi.org", latestVersion: "1.2.3" });
    const pipCommand = getInstallCommand(pip);
    expect(pipCommand).toContain("install");
    expect(pipCommand).toContain("demo==1.2.3");
    expect(pipCommand).toContain("--index-url");

    const conda = result({ source: "conda", repository: "conda-forge" });
    const condaCommand = getInstallCommand(conda);
    expect(condaCommand).toContain("conda");
    expect(condaCommand).toContain("-c");
  });
});
