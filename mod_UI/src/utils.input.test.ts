import { describe, it, expect } from "vitest";
import {
  isActiveInputLine,
  nonEmptyLineCountExceeds,
  extractCanonicalInput,
  dedupePackageInput,
  normalizePackageInputDisplay,
  parseProjectDependencyFile,
  extractSystemRequirements,
  trimTrailingBlankLines,
  classifyInputProfile,
  countDuplicatePackages,
  cleanPackageInput,
  collectBrowserSearchNames,
  isInstallArchiveUrl,
  isGithubRepositoryUrl,
  activeInputLineCount,
} from "./utils";
import { defaultInputRules } from "./types";
const withSeparators = (separators: string[]) => ({ ...defaultInputRules, separators });

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
    expect(classifyInputProfile("dplyr|ggplot2", withSeparators(["|"]))).toEqual({
      total: 2,
      archiveUrls: 0,
      repositories: 0,
    });
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

describe("countDuplicatePackages", () => {
  it("counts duplicates case-insensitively", () => {
    expect(countDuplicatePackages("dplyr\nDPLYR\nggplot2")).toBe(1);
  });

  it("returns 0 when no duplicates", () => {
    expect(countDuplicatePackages("dplyr\nggplot2")).toBe(0);
  });
});

describe("URL 输入分类（与后端 input.rs / url_validation.rs 对齐）", () => {
  it("GitHub 仓库 URL 归入 repositories，不计入 archiveUrls", () => {
    expect(classifyInputProfile("https://github.com/tidyverse/dplyr")).toEqual({
      total: 1,
      archiveUrls: 0,
      repositories: 1,
    });
  });

  it("接受 .git 后缀的 GitHub 仓库 URL", () => {
    expect(isGithubRepositoryUrl("https://github.com/tidyverse/dplyr.git")).toBe(true);
    expect(classifyInputProfile("https://github.com/tidyverse/dplyr.git")).toEqual({
      total: 1,
      archiveUrls: 0,
      repositories: 1,
    });
  });

  it("拒绝带子路径、查询参数、片段、凭据或端口的 GitHub URL", () => {
    expect(isGithubRepositoryUrl("https://github.com/tidyverse/dplyr/tree/main")).toBe(false);
    expect(isGithubRepositoryUrl("https://github.com/tidyverse/dplyr?tab=readme")).toBe(false);
    expect(isGithubRepositoryUrl("https://github.com/tidyverse/dplyr#readme")).toBe(false);
    expect(isGithubRepositoryUrl("https://user:pass@github.com/tidyverse/dplyr")).toBe(false);
    expect(isGithubRepositoryUrl("https://github.com:8443/tidyverse/dplyr")).toBe(false);
    expect(isGithubRepositoryUrl("http://github.com/tidyverse/dplyr")).toBe(false);
  });

  it("归档 URL 归入 archiveUrls", () => {
    expect(classifyInputProfile("https://x.example/pkg_1.0.tar.gz")).toEqual({
      total: 1,
      archiveUrls: 1,
      repositories: 0,
    });
    expect(isInstallArchiveUrl("http://192.168.5.250:8011/pkg_1.0.zip")).toBe(true);
  });

  it("非归档 http(s) 行不计入 total", () => {
    const value = [
      "https://x.example/index.html",
      "https://x.example/pkg_1.0.tar.gz?token=abc",
      "https://x.example/pkg_1.0.tar.gz#frag",
      "https://user:pass@x.example/pkg_1.0.tar.gz",
    ].join("\n");
    expect(classifyInputProfile(value)).toEqual({ total: 0, archiveUrls: 0, repositories: 0 });
    expect(isInstallArchiveUrl("https://x.example/index.html")).toBe(false);
    expect(isInstallArchiveUrl("https://x.example/pkg_1.0.tar.gz?token=abc")).toBe(false);
  });

  it("混合输入分别归类", () => {
    expect(
      classifyInputProfile(
        ["dplyr", "https://github.com/tidyverse/dplyr", "https://x.example/pkg_1.0.tgz"].join("\n"),
      ),
    ).toEqual({ total: 3, archiveUrls: 1, repositories: 1 });
  });

  it("extractCanonicalInput 只保留可识别的 http(s) 行", () => {
    expect(
      extractCanonicalInput(
        ["https://github.com/tidyverse/dplyr", "https://x.example/index.html"].join("\n"),
      ),
    ).toBe("https://github.com/tidyverse/dplyr");
  });
});

describe("cleanPackageInput", () => {
  it("去除首尾空白与空行并统一全角分隔符", () => {
    expect(cleanPackageInput("  dplyr，tidyr  \n\n  ggplot2  \n")).toBe("dplyr,tidyr\nggplot2");
  });

  it("保留注释行", () => {
    expect(cleanPackageInput("# 注释\n\n  dplyr  ")).toBe("# 注释\ndplyr");
  });

  it("仍会规范化 Markdown 表格", () => {
    expect(cleanPackageInput("| 包名 | 来源 |\n| --- | --- |\n| DMwR | CRAN |")).toBe("DMwR");
  });
});

describe("collectBrowserSearchNames", () => {
  it("拒绝以数字开头的包名（对齐 R 包名规范）", () => {
    expect(collectBrowserSearchNames("3Dpack\ndplyr", 10).names).toEqual(["dplyr"]);
  });

  it("从仓库路径中提取包名", () => {
    expect(collectBrowserSearchNames("tidyverse/dplyr", 10).names).toEqual(["dplyr"]);
  });
});

describe("全角标点归一化（与 Rust split_by_separators 对齐）", () => {
  it("全角分号与全角逗号、顿号一样映射为半角逗号", () => {
    // Rust `input.rs::split_by_separators` 用 replace(['，','、','；'], ",")，
    // 前端若把 `；` 映射为 `;`，在用户只把 `,` 配成分隔符时两侧包数会对不上。
    expect(classifyInputProfile("dplyr；ggplot2", withSeparators([","]))).toEqual({
      total: 2,
      archiveUrls: 0,
      repositories: 0,
    });
    expect(classifyInputProfile("dplyr、ggplot2，tidyr", withSeparators([","])).total).toBe(3);
  });

  it("显式清理与显示规范化对全角标点保持同一口径", () => {
    expect(cleanPackageInput("dplyr；ggplot2")).toBe("dplyr,ggplot2");
  });
});

describe("输入过滤规则镜像（与 Rust parse_inputs_filtered 同口径）", () => {
  it("注释字符按配置生效，而不是固定 #", () => {
    const rules = { ...defaultInputRules, commentChars: ["#", "//"] };
    expect(activeInputLineCount("//dplyr\n#tidyr\nggplot2", rules)).toBe(1);
    expect(classifyInputProfile("//dplyr\nggplot2", rules).total).toBe(1);
    // 未把 `//` 配成注释字符时，它就是普通包名
    expect(activeInputLineCount("//dplyr\n#tidyr\nggplot2", defaultInputRules)).toBe(2);
    // 注释字符为空数组时不把任何行当注释
    expect(activeInputLineCount("#dplyr\nggplot2", { ...defaultInputRules, commentChars: [] })).toBe(2);
  });

  it("排除关键词按包名（不区分大小写）剔除", () => {
    const rules = { ...defaultInputRules, excludeKeywords: ["dplyr"] };
    expect(classifyInputProfile("dplyr\nggplot2", rules).total).toBe(1);
    expect(activeInputLineCount("DPlyr\nggplot2", rules)).toBe(1);
  });

  it("排除正则同时作用于整行与拆分后的每一段", () => {
    const lineLevel = { ...defaultInputRules, excludeRegex: ["^library\\("] };
    expect(classifyInputProfile("library(dplyr)\nggplot2", lineLevel).total).toBe(1);
    const segmentLevel = { ...defaultInputRules, excludeRegex: ["^ggplot"] };
    expect(classifyInputProfile("dplyr,ggplot2", segmentLevel).total).toBe(1);
  });

  it("关闭引号剥离 / c() 剥离后行为与后端一致", () => {
    expect(dedupePackageInput('"dplyr"\ndplyr', defaultInputRules)).toBe("dplyr");
    const keepQuotes = { ...defaultInputRules, stripQuotes: false };
    expect(dedupePackageInput('"dplyr"\ndplyr', keepQuotes)).toBe('"dplyr"\ndplyr');
    expect(activeInputLineCount("c(dplyr, ggplot2)", defaultInputRules)).toBe(2);
    // 剥离 c() 后 `c(dplyr)` 与裸 `dplyr` 视为同一个包；关闭剥离时两者不同。
    expect(dedupePackageInput("c(dplyr)\ndplyr", defaultInputRules)).toBe("dplyr");
    const keepParens = { ...defaultInputRules, stripCParens: false };
    expect(dedupePackageInput("c(dplyr)\ndplyr", keepParens)).toBe("c(dplyr)\ndplyr");
  });

  it("空格分割开关与后端 split_spaces 一致", () => {
    expect(activeInputLineCount("dplyr ggplot2 tidyr", defaultInputRules)).toBe(1);
    expect(activeInputLineCount("dplyr ggplot2 tidyr", { ...defaultInputRules, splitSpaces: true })).toBe(3);
  });

  it("R 调用包装前缀表与后端 strip_r_parens_wrapper 一致", () => {
    expect(classifyInputProfile('install.packages("dplyr")').total).toBe(1);
    expect(classifyInputProfile("library(dplyr, ggplot2)").total).toBe(2);
    expect(classifyInputProfile('BiocManager::install("edgeR")').total).toBe(1);
  });
});
