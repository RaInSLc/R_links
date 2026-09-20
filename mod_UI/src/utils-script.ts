import { MAX_SCRIPT_CHARS } from "./utils-types";
import { utf8Length } from "./utils-sanitize";

const PACKAGE_DECLARATION_RE =
  /^[A-Za-z0-9][A-Za-z0-9._-]*(?:(?:==|!=|>=|<=|~=|>|<|=)[0-9]+(?:\.[0-9]+)*(?:,(?:==|!=|>=|<=|~=|>|<|=)[0-9]+(?:\.[0-9]+)*)*)?$/;
const SYSTEM_REQUIREMENT_PACKAGES: Record<string, [string, string, string]> = {
  gdal: ["libgdal-dev", "gdal-devel", "gdal"],
  geos: ["libgeos-dev", "geos-devel", "geos"],
  proj: ["libproj-dev", "proj-devel", "proj"],
  xml: ["libxml2-dev", "libxml2-devel", "libxml2"],
  ssl: ["libssl-dev", "openssl-devel", "openssl@3"],
  gsl: ["libgsl-dev", "gsl-devel", "gsl"],
  curl: ["libcurl4-openssl-dev", "libcurl-devel", "curl"],
};
const PIP_INDEX_DEFAULT = "https://pypi.org";
const PYTHON_PACKAGE_COMMAND_RE = new RegExp(
  "\\b(?:install\\.packages|BiocManager::install|remotes::install_|devtools::install_|install_url|install_github|packageVersion|library|require|cat|stop|message|warning)\\s*\\(",
);
const CONDA_CHANNEL_RE = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

export function parseMultiRequirements(input: string): string[] {
  return input
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"))
    .map((line) => {
      const value = line
        .replace(/^(["'])(.*)\1$/, "$2")
        .replace(/\s+/g, "");
      if (!PACKAGE_DECLARATION_RE.test(value)) {
        throw new Error("包声明格式不支持：请使用包名及数字版本约束，不支持命令选项或 Shell 表达式");
      }
      return value;
    });
}

function resolvePipIndex(pipIndex: string): string {
  const configured = pipIndex.trim().replace(/\/+$/, "");
  return configured === PIP_INDEX_DEFAULT ? `${configured}/simple` : configured;
}

function assertPipIndexSafe(index: string) {
  if (!index) return;
  const url = new URL(index);
  if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash) {
    throw new Error("Pip 索引必须是不含凭据、查询参数和片段的 HTTPS 地址");
  }
}

function assertCondaChannelsValid(channels: string[]) {
  if (channels.some((channel) => !CONDA_CHANNEL_RE.test(channel))) {
    throw new Error("Conda 渠道名称无效");
  }
}

function quoteShell(value: string, shell: "bash" | "powershell"): string {
  if (shell === "powershell") {
    return `'${value.replace(/'/g, "''")}'`;
  }
  return `'${value.replace(/'/g, "'\\''")}'`;
}

function buildPipInstallArgs(requirement: string, index: string): string[] {
  const args = ["install", requirement];
  if (index) {
    args.push("--index-url", index);
  }
  return args;
}

function buildCondaInstallArgs(requirement: string, channels: string[]): string[] {
  const args = ["install"];
  for (const channel of channels) {
    args.push("-c", channel);
  }
  args.push(requirement);
  return args;
}

function buildCommandLine(ecosystem: "pip" | "conda", args: string[], shell: "bash" | "powershell"): string {
  const prefix = shell === "powershell" ? "& " : "";
  const suffix = shell === "powershell" ? "\nif ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }" : "";
  return `${prefix}${ecosystem} ${args.map((arg) => quoteShell(arg, shell)).join(" ")}${suffix}`;
}

function buildScriptHeader(shell: "bash" | "powershell"): string {
  if (shell === "powershell") {
    return '$ErrorActionPreference = "Stop"';
  }
  return "#!/usr/bin/env bash\nset -e";
}

export function generateMultiEcosystemScript(
  input: string,
  ecosystem: "pip" | "conda",
  pipIndex: string,
  condaChannels: string[],
  shell: "bash" | "powershell" = "bash",
) {
  const packages = parseMultiRequirements(input);
  if (!packages.length) return "等待输入...";
  const index = resolvePipIndex(pipIndex);
  assertPipIndexSafe(index);
  const channels = condaChannels.map((channel) => channel.trim()).filter(Boolean);
  assertCondaChannelsValid(channels);
  const commands = packages.map((value) => {
    const requirement = ecosystem === "pip" ? value.replace(/(?<![=!<>~])=(?!=)/g, "==") : value;
    if (ecosystem === "conda" && requirement.includes("~=")) {
      throw new Error("Conda 暂不支持兼容版本运算符 ~=，请使用 >= 和 < 范围");
    }
    const args = ecosystem === "pip"
      ? buildPipInstallArgs(requirement, index)
      : buildCondaInstallArgs(requirement, channels);
    return buildCommandLine(ecosystem, args, shell);
  });
  const script = `${buildScriptHeader(shell)}\n${commands.join("\n")}\n`;
  if (scriptValueTooLarge(script)) throw new Error("生成脚本超出大小限制");
  return script;
}

function detectSystemRequirements(names: Set<string>): { requirements: Set<string>; windows: Set<string> } {
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
  return { requirements, windows };
}

function buildNativeInstallCommand(manager: string, column: number, requirements: Set<string>): string {
  if (!requirements.size) return "echo 'No known system requirements detected.'";
  const names = [...requirements].map((name) => SYSTEM_REQUIREMENT_PACKAGES[name][column]).join(" ");
  return `${manager} ${names}`;
}

function buildBashSystemScript(requirements: Set<string>): string {
  const apt = buildNativeInstallCommand("sudo apt-get install -y", 0, requirements);
  const dnf = buildNativeInstallCommand("sudo dnf install -y", 1, requirements);
  const yum = buildNativeInstallCommand("sudo yum install -y", 1, requirements);
  const brew = buildNativeInstallCommand("brew install", 2, requirements);
  const branches = [
    "if command -v apt-get >/dev/null 2>&1; then",
    "  sudo apt-get update",
    `  ${apt}`,
    "elif command -v dnf >/dev/null 2>&1; then",
    `  ${dnf}`,
    "elif command -v yum >/dev/null 2>&1; then",
    `  ${yum}`,
    "elif command -v brew >/dev/null 2>&1; then",
    `  ${brew}`,
    "else",
    "  echo 'No supported package manager detected; install system requirements manually.'",
    "fi",
  ];
  const body = branches.join("\n");
  return `#!/usr/bin/env bash\nset -e\n# 根据当前包管理器选择对应的原生依赖名称。\n${body}\n`;
}

function buildPowerShellSystemScript(windows: Set<string>): string {
  const detected = [...windows];
  if (!detected.length) {
    return `# Windows 原生依赖需要人工确认安装。\n$ErrorActionPreference = "Stop"\nWrite-Host 'No known system requirements detected.'\n`;
  }
  const listLiteral = detected.map((item) => `"${item}"`).join(", ");
  const lines = [
    "# Windows 原生依赖需要人工确认安装。",
    '$ErrorActionPreference = "Stop"',
    `$requirements = @(${listLiteral})`,
    'Write-Host "Detected native requirements:"',
    "$requirements | ForEach-Object { Write-Host \" - $_\" }",
    'Write-Host "Install the listed components manually; they are not Chocolatey package identifiers."',
  ];
  return `${lines.join("\n")}\n`;
}

export function generateSystemRequirementsScript(input: string, kind: "bash" | "powershell") {
  const names = new Set((input.match(/[A-Za-z][A-Za-z0-9._]*/g) || []).map((name) => name.toLowerCase()));
  const { requirements, windows } = detectSystemRequirements(names);
  if (kind === "bash") return buildBashSystemScript(requirements);
  return buildPowerShellSystemScript(windows);
}

export const scriptValueTooLarge = (value: string) =>
  value.length > MAX_SCRIPT_CHARS || utf8Length(value) > MAX_SCRIPT_CHARS;

export function countScriptCommands(script: string) {
  if (!script || script === "等待输入...") return 0;
  return script
    .split(/\r?\n/)
    .filter((line) => {
      const trimmed = line.trim();
      return trimmed && !trimmed.startsWith("#") && PYTHON_PACKAGE_COMMAND_RE.test(trimmed);
    })
    .length;
}