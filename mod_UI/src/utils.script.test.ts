import { describe, it, expect } from "vitest";
import {
  generateMultiEcosystemScript,
  generateSystemRequirementsScript,
  countScriptCommands,
} from "./utils";

describe("generateMultiEcosystemScript", () => {
  it("保留范围约束并拒绝命令表达式", () => {
    expect(generateMultiEcosystemScript("numpy>=1.26,<2", "pip", "https://pypi.org/simple", [])).toContain("'numpy>=1.26,<2'");
    expect(() => generateMultiEcosystemScript("numpy;echo test", "pip", "", [])).toThrow();
    expect(() => generateMultiEcosystemScript("numpy$(echo test)", "pip", "", [])).toThrow();
  });
  it("PowerShell 使用独立生成器并传播退出码", () => {
    const script = generateMultiEcosystemScript("numpy>=1.26", "pip", "https://pypi.org/simple", [], "powershell");
    expect(script).toContain("& pip 'install' 'numpy>=1.26'");
    expect(script).toContain("exit $LASTEXITCODE");
    expect(script).not.toContain("#!/");
    expect(script).not.toContain("set -e");
  });
  it("系统依赖根据发行版选择独立包名", () => {
    const script = generateSystemRequirementsScript("sf 1.0\nopenssl", "bash");
    expect(script).toContain("sudo apt-get install -y libgdal-dev libgeos-dev libproj-dev libssl-dev");
    expect(script).toContain("sudo yum install -y gdal-devel geos-devel proj-devel openssl-devel");
    expect(script).not.toContain("sudo yum install -y libgdal-dev");
  });
  it("generates pip install commands with the configured index", () => {
    expect(generateMultiEcosystemScript("numpy==1.26\npandas", "pip", "https://pypi.org/simple", [])).toContain("pip 'install' 'numpy==1.26' '--index-url' 'https://pypi.org/simple'");
  });

  it("generates conda install commands with configured channels", () => {
    expect(generateMultiEcosystemScript("numpy=1.26", "conda", "", ["conda-forge", "bioconda"])).toContain("conda 'install' '-c' 'conda-forge' '-c' 'bioconda' 'numpy=1.26'");
  });
});

describe("generateSystemRequirementsScript", () => {
  it("generates Linux dependency preparation commands", () => {
    expect(generateSystemRequirementsScript("sf\nxml2", "bash")).toContain("libgdal-dev");
    expect(generateSystemRequirementsScript("sf\nxml2", "bash")).toContain("libxml2-dev");
  });

  it("generates a reviewable Windows preparation script", () => {
    const script = generateSystemRequirementsScript("sf", "powershell");
    expect(script).toContain("Rtools");
    expect(script).toContain("OSGeo4W");
    expect(script).not.toContain("choco install");
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
