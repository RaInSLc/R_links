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
} from "./utils";

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
    expect(classifyInputProfile("dplyr|ggplot2", ["|"])).toEqual({
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
