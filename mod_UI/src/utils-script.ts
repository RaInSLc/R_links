import { MAX_SCRIPT_CHARS } from "./utils-types";
import { utf8Length } from "./utils-sanitize";
export function parseMultiRequirements(input: string): string[] {
  return input.split(/\r?\n/).map((line) => line.trim()).filter((line) => line && !line.startsWith("#")).map((line) => {
    const value = line.replace(/^(["'])(.*)\1$/, "$2").replace(/\s+/g, "");
    if (!/^[A-Za-z0-9][A-Za-z0-9._-]*(?:(?:==|!=|>=|<=|~=|>|<|=)[0-9]+(?:\.[0-9]+)*(?:,(?:==|!=|>=|<=|~=|>|<|=)[0-9]+(?:\.[0-9]+)*)*)?$/.test(value)) {
      throw new Error("包声明格式不支持：请使用包名及数字版本约束，不支持命令选项或 Shell 表达式");
    }
    return value;
  });
}

export function generateMultiEcosystemScript(input: string, ecosystem: "pip" | "conda", pipIndex: string, condaChannels: string[], shell: "bash" | "powershell" = "bash") {
  const packages = parseMultiRequirements(input);
  if (!packages.length) return "等待输入...";
  const configuredIndex = pipIndex.trim().replace(/\/+$/, "");
  const index = configuredIndex === "https://pypi.org" ? `${configuredIndex}/simple` : configuredIndex;
  if (index) {
    const url = new URL(index);
    if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash) throw new Error("Pip 索引必须是不含凭据、查询参数和片段的 HTTPS 地址");
  }
  const channels = condaChannels.map((channel) => channel.trim()).filter(Boolean);
  if (channels.some((channel) => !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(channel))) throw new Error("Conda 渠道名称无效");
  const quote = (value: string) => shell === "powershell" ? `'${value.replace(/'/g, "''")}'` : `'${value.replace(/'/g, "'\\''")}'`;
  const commands = packages.map((value) => {
    const requirement = ecosystem === "pip" ? value.replace(/(?<![=!<>~])=(?!=)/g, "==") : value;
    if (ecosystem === "conda" && requirement.includes("~=")) throw new Error("Conda 暂不支持兼容版本运算符 ~=，请使用 >= 和 < 范围");
    const args = ecosystem === "pip" ? ["install", requirement, ...(index ? ["--index-url", index] : [])] : ["install", ...channels.flatMap((channel) => ["-c", channel]), requirement];
    return `${shell === "powershell" ? "& " : ""}${ecosystem} ${args.map(quote).join(" ")}${shell === "powershell" ? "\nif ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }" : ""}`;
  });
  const script = `${shell === "powershell" ? '$ErrorActionPreference = "Stop"' : "#!/usr/bin/env bash\nset -e"}\n${commands.join("\n")}\n`;
  if (scriptValueTooLarge(script)) throw new Error("生成脚本超出大小限制");
  return script;
}
export function generateSystemRequirementsScript(input: string, kind: "bash" | "powershell") {
  const names = new Set((input.match(/[A-Za-z][A-Za-z0-9._]*/g) || []).map((name) => name.toLowerCase()));
  const requirements = new Set<string>();
  const windows = new Set<string>();
  if (["sf", "terra", "rgdal", "rgeos"].some((name) => names.has(name))) {
    ["gdal", "geos", "proj"].forEach((name) => requirements.add(name));
    windows.add("Rtools (matching the installed R version)");
    windows.add("OSGeo4W (GDAL/GEOS/PROJ)");
  }
  if (["xml2", "rvest"].some((name) => names.has(name))) requirements.add("xml");
  if (["curl", "httr"].some((name) => names.has(name))) requirements.add("curl");
  if (["openssl", "httr", "curl"].some((name) => names.has(name))) requirements.add("ssl");
  if (names.has("gsl")) requirements.add("gsl");
  const packages: Record<string, [string, string, string]> = {
    gdal: ["libgdal-dev", "gdal-devel", "gdal"], geos: ["libgeos-dev", "geos-devel", "geos"], proj: ["libproj-dev", "proj-devel", "proj"],
    xml: ["libxml2-dev", "libxml2-devel", "libxml2"], ssl: ["libssl-dev", "openssl-devel", "openssl@3"], gsl: ["libgsl-dev", "gsl-devel", "gsl"], curl: ["libcurl4-openssl-dev", "libcurl-devel", "curl"],
  };
  const install = (manager: string, column: number) => requirements.size ? `${manager} ${[...requirements].map((name) => packages[name][column]).join(" ")}` : "echo 'No known system requirements detected.'";
  if (kind === "bash") return `#!/usr/bin/env bash\nset -e\n# 根据当前包管理器选择对应的原生依赖名称。\nif command -v apt-get >/dev/null 2>&1; then\n  sudo apt-get update\n  ${install("sudo apt-get install -y", 0)}\nelif command -v dnf >/dev/null 2>&1; then\n  ${install("sudo dnf install -y", 1)}\nelif command -v yum >/dev/null 2>&1; then\n  ${install("sudo yum install -y", 1)}\nelif command -v brew >/dev/null 2>&1; then\n  ${install("brew install", 2)}\nelse\n  echo 'No supported package manager detected; install system requirements manually.'\nfi\n`;
  const detected = [...windows];
  return `# Windows 原生依赖需要人工确认安装。\n$ErrorActionPreference = "Stop"\n${detected.length ? `$requirements = @(${detected.map((item) => `"${item}"`).join(", ")})\nWrite-Host "Detected native requirements:"\n$requirements | ForEach-Object { Write-Host " - $_" }\nWrite-Host "Install the listed components manually; they are not Chocolatey package identifiers."` : "Write-Host 'No known system requirements detected.'"}\n`;
}
export const scriptValueTooLarge = (value: string) => value.length > MAX_SCRIPT_CHARS || utf8Length(value) > MAX_SCRIPT_CHARS;
export function countScriptCommands(script: string) { if (!script || script === "等待输入...") return 0; return script.split(/\r?\n/).filter((l) => { const s = l.trim(); return s && !s.startsWith("#") && /\b(?:install\.packages|BiocManager::install|remotes::install_|devtools::install_|install_url|install_github|packageVersion|library|require|cat|stop|message|warning)\s*\(/.test(s); }).length; }
