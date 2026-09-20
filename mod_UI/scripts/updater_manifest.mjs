import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const BASE64_ALPHABET = /^[A-Za-z0-9+/]+={0,2}$/;

function decodeBase64Strict(value, label) {
  const trimmed = value.trim();
  if (!BASE64_ALPHABET.test(trimmed) || trimmed.length % 4 !== 0) {
    throw new Error(`${label} 不是合法的 base64（长度 ${trimmed.length}）`);
  }
  return Buffer.from(trimmed, "base64");
}

/**
 * 取出 minisign 公钥/签名里的密钥 ID，解码链路与
 * `tauri-plugin-updater::verify_signature` 完全一致：
 * 外层 base64 解出文本 → 第二行再 base64 解出二进制 → `[2..10]` 为 key id。
 *
 * 算法标识两者都要接受：`Ed` 为普通签名，`ED` 为预哈希签名——Tauri 的
 * `tauri signer sign` 输出的正是 `ED`，只认 `Ed` 会把合法签名判为非法。
 */
export function minisignKeyId(encoded, label = "minisign 数据") {
  const text = decodeBase64Strict(encoded, label).toString("utf8");
  const body = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("untrusted comment"))[0];
  if (!body) throw new Error(`${label} 缺少公钥/签名主体行`);
  const raw = decodeBase64Strict(body, `${label} 主体`);
  const algorithm = raw.subarray(0, 2).toString("latin1");
  if (algorithm !== "Ed" && algorithm !== "ED") {
    throw new Error(`${label} 的算法标识不是 Ed/ED（实际为 ${JSON.stringify(algorithm)}）`);
  }
  return raw.subarray(2, 10).toString("hex").toUpperCase();
}

export function readTauriConfig() {
  const configPath = path.join(projectRoot, "src-tauri", "tauri.conf.json");
  const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
  const updater = config.plugins?.updater ?? {};
  return {
    configPath,
    version: String(config.version),
    pubkey: String(updater.pubkey ?? ""),
    endpoints: Array.isArray(updater.endpoints) ? updater.endpoints : [],
    createUpdaterArtifacts: config.bundle?.createUpdaterArtifacts === true,
    productName: String(config.productName ?? ""),
  };
}

/** 从更新端点反推 GitHub 仓库，避免在多处重复写 owner/repo。 */
export function repoFromEndpoint(endpoint) {
  const match = /^https:\/\/github\.com\/([^/]+)\/([^/]+)\/releases\//.exec(endpoint ?? "");
  return match ? { owner: match[1], repo: match[2] } : null;
}

export function bundleDirs() {
  const release = path.join(projectRoot, "src-tauri", "target", "release");
  return {
    release,
    nsis: path.join(release, "bundle", "nsis"),
    msi: path.join(release, "bundle", "msi"),
  };
}

/** 收集 `*.sig` 与对应的安装包，判定更新产物的目标（NSIS 优先）。 */
export function collectUpdaterArtifacts() {
  const dirs = bundleDirs();
  const found = [];
  for (const [kind, dir] of [["nsis", dirs.nsis], ["msi", dirs.msi]]) {
    if (!fs.existsSync(dir)) continue;
    for (const entry of fs.readdirSync(dir)) {
      if (!entry.endsWith(".sig")) continue;
      const installer = path.join(dir, entry.slice(0, -4));
      if (!fs.existsSync(installer)) continue;
      found.push({ kind, installer, signature: path.join(dir, entry) });
    }
  }
  return found;
}

/** GitHub 上的资产名把空格换成点（历史发布的实际命名）。 */
export function releaseAssetName(fileName) {
  return fileName.replace(/ /g, ".");
}

export function buildUpdaterManifest({ version, tag, owner, repo, artifact, notes, pubDate }) {
  const signature = fs.readFileSync(artifact.signature, "utf8").trim();
  const assetName = releaseAssetName(path.basename(artifact.installer));
  return {
    version,
    notes: notes ?? `R Package Command Center ${tag}`,
    pub_date: pubDate ?? new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
    platforms: {
      "windows-x86_64": {
        signature,
        url: `https://github.com/${owner}/${repo}/releases/download/${tag}/${assetName}`,
      },
    },
  };
}

/** 校验清单结构；返回问题列表，空数组表示通过。 */
export function validateUpdaterManifest(manifest, { expectedVersion }) {
  const problems = [];
  if (!manifest || typeof manifest !== "object") return ["清单不是 JSON 对象"];
  if (!manifest.version) problems.push("缺少 version");
  if (expectedVersion && manifest.version !== expectedVersion) {
    problems.push(`version=${manifest.version} 与预期 ${expectedVersion} 不一致`);
  }
  if (!manifest.pub_date) problems.push("缺少 pub_date");
  const platforms = manifest.platforms;
  if (!platforms || typeof platforms !== "object" || Object.keys(platforms).length === 0) {
    problems.push("缺少 platforms");
    return problems;
  }
  for (const [name, platform] of Object.entries(platforms)) {
    if (!platform?.url) problems.push(`platforms.${name} 缺少 url`);
    if (!platform?.signature) problems.push(`platforms.${name} 缺少 signature`);
    if (platform?.url && !platform.url.startsWith("https://")) {
      problems.push(`platforms.${name} 的 url 不是 https`);
    }
  }
  return problems;
}
