/**
 * 输入 URL 分类工具。
 *
 * 判定规则必须与 Rust 侧保持一致：
 * - GitHub 仓库：`url_validation.rs::normalize_github_repository` / `github_repository_from_url`
 * - 安装归档：`url_validation.rs::normalize_install_archive_url`
 * 任一侧调整规则时，必须同步修改另一侧并补充回归测试。
 */

const INSTALL_ARCHIVE_EXTENSIONS = [".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".zip"];
const MAX_INSTALL_ARCHIVE_FILE_CHARS = 256;
const MAX_FIELD_CHARS = 2048;
const MAX_GITHUB_OWNER_CHARS = 39;
const MAX_GITHUB_REPO_CHARS = 100;
const MAX_GITHUB_REPOSITORY_CHARS = 200;

/** 与 Rust `char::is_control()` 对齐：仅 Unicode Cc 类别。 */
const CONTROL_CHARACTER_RE = /[\p{Cc}]/u;

export type UrlInputKind = "archive" | "repository" | "invalid";

function parseUrl(value: string): URL | null {
  try {
    return new URL(value);
  } catch {
    return null;
  }
}

/** 对齐 Rust `url_has_explicit_port`：URL 字符串中是否显式书写了端口。 */
function hasExplicitPort(value: string): boolean {
  const schemeIndex = value.indexOf("://");
  if (schemeIndex < 0) {
    return false;
  }
  const authorityStart = schemeIndex + 3;
  let authorityEnd = value.length;
  for (const delimiter of ["/", "?", "#"]) {
    const index = value.indexOf(delimiter, authorityStart);
    if (index >= 0 && index < authorityEnd) {
      authorityEnd = index;
    }
  }
  const authority = value.slice(authorityStart, authorityEnd);
  const atIndex = authority.lastIndexOf("@");
  const hostPort = atIndex >= 0 ? authority.slice(atIndex + 1) : authority;
  if (hostPort.startsWith("[")) {
    const closeIndex = hostPort.indexOf("]");
    return closeIndex >= 0 && hostPort.slice(closeIndex + 1).startsWith(":");
  }
  return hostPort.includes(":");
}

function isGithubOwnerSegment(value: string): boolean {
  return (
    value.length > 0 &&
    value.length <= MAX_GITHUB_OWNER_CHARS &&
    !value.startsWith("-") &&
    !value.endsWith("-") &&
    !value.includes("--") &&
    /^[A-Za-z0-9-]+$/.test(value)
  );
}

function isGithubRepoSegment(value: string): boolean {
  return value.length > 0 && value.length <= MAX_GITHUB_REPO_CHARS && /^[A-Za-z0-9._-]+$/.test(value);
}

/** 对齐 Rust `is_valid_github_repository`，`owner/repo[/子路径...]`。 */
export function isGithubRepositoryName(value: string): boolean {
  if (value.length > MAX_GITHUB_REPOSITORY_CHARS || value.includes("\\") || value.includes("..")) {
    return false;
  }
  const parts = value.replace(/^\/+/, "").replace(/\/+$/, "").split("/");
  if (parts.length < 2) {
    return false;
  }
  if (!isGithubOwnerSegment(parts[0]) || !isGithubRepoSegment(parts[1])) {
    return false;
  }
  return parts.slice(2).every(isGithubRepoSegment);
}

/** 对齐 Rust `github_repository_from_url`：仅接受 https://github.com/owner/repo（可带 .git）。 */
export function isGithubRepositoryUrl(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed.includes("://")) {
    return false;
  }
  const parsed = parseUrl(trimmed);
  if (!parsed) {
    return false;
  }
  if (parsed.protocol !== "https:" || parsed.hostname !== "github.com") {
    return false;
  }
  if (parsed.username || parsed.password) {
    return false;
  }
  if (parsed.port || hasExplicitPort(trimmed)) {
    return false;
  }
  if (parsed.search || parsed.hash) {
    return false;
  }
  const segments = parsed.pathname.split("/").filter((segment) => segment.length > 0);
  if (segments.length !== 2) {
    return false;
  }
  const repository = `${segments[0]}/${segments[1].replace(/\.git$/, "")}`;
  return isGithubRepositoryName(repository);
}

/** 对齐 Rust `normalize_install_archive_url`：http(s) + 主机 + 无凭据/查询/片段 + 归档扩展名。 */
export function isInstallArchiveUrl(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed || trimmed.length > MAX_FIELD_CHARS || CONTROL_CHARACTER_RE.test(trimmed)) {
    return false;
  }
  const parsed = parseUrl(trimmed);
  if (!parsed) {
    return false;
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    return false;
  }
  if (!parsed.hostname) {
    return false;
  }
  if (parsed.username || parsed.password) {
    return false;
  }
  if (parsed.search || parsed.hash) {
    return false;
  }
  const segments = parsed.pathname.split("/");
  const fileName = segments[segments.length - 1] ?? "";
  if (!fileName || fileName.length > MAX_INSTALL_ARCHIVE_FILE_CHARS || CONTROL_CHARACTER_RE.test(fileName)) {
    return false;
  }
  const lowerFileName = fileName.toLowerCase();
  return INSTALL_ARCHIVE_EXTENSIONS.some((extension) => lowerFileName.endsWith(extension));
}

/** 三分类：GitHub 仓库 / 安装归档 / 非法（后端 `parse_inputs_filtered` 会整批拒绝非法行）。 */
export function classifyUrlInput(value: string): UrlInputKind {
  const trimmed = value.trim();
  if (!/^https?:\/\//i.test(trimmed)) {
    return "invalid";
  }
  if (isGithubRepositoryUrl(trimmed)) {
    return "repository";
  }
  if (isInstallArchiveUrl(trimmed)) {
    return "archive";
  }
  return "invalid";
}

/** 是否为后端能够识别的 http(s) 输入行。 */
export function isRecognizedUrlInput(value: string): boolean {
  return classifyUrlInput(value) !== "invalid";
}
