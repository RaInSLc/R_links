import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import {
  collectUpdaterArtifacts,
  minisignKeyId,
  projectRoot,
  readTauriConfig,
  validateUpdaterManifest,
} from "./updater_manifest.mjs";

const argv = process.argv.slice(2);
const preRelease = argv.includes("--pre-release");

const results = [];
let failed = false;
const record = (ok, label, detail) => {
  results.push({ ok, label, detail });
  if (!ok) failed = true;
};

/**
 * 逐环验证「点检查更新 → 下载 → 验签 → 安装」这条链路是否闭环。
 * 这一环此前完全缺失：配置写坏、清单没生成、密钥不匹配都不会让
 * 构建或发布报错，用户端只会看到一句笼统的失败提示。
 */
function verifyConfig(config) {
  let keyId = null;
  try {
    keyId = minisignKeyId(config.pubkey, "updater.pubkey");
    record(true, "签名公钥可解析", `key id ${keyId}`);
  } catch (error) {
    record(false, "签名公钥可解析", `${error.message}；该公钥下任何更新包都无法通过验签`);
  }
  record(
    config.createUpdaterArtifacts,
    "bundle.createUpdaterArtifacts = true",
    config.createUpdaterArtifacts ? "会产出 .sig 与 latest.json" : "不会产出 .sig，发布流程无法生成 latest.json",
  );
  record(config.endpoints.length > 0, "已配置更新端点", config.endpoints[0] ?? "缺少 plugins.updater.endpoints");
  return keyId;
}

function verifyLocalArtifacts(config, keyId) {
  const artifacts = collectUpdaterArtifacts();
  if (artifacts.length === 0) {
    console.log("本地尚无 .sig 产物（尚未做签名构建），跳过本地签名校验。");
    return;
  }
  for (const artifact of artifacts) {
    const name = path.basename(artifact.installer);
    try {
      const sigKeyId = minisignKeyId(fs.readFileSync(artifact.signature, "utf8"), `${name} 的签名`);
      record(
        !keyId || sigKeyId === keyId,
        `本地签名与公钥匹配（${name}）`,
        `签名 key id ${sigKeyId}${keyId ? ` / 公钥 ${keyId}` : ""}`,
      );
    } catch (error) {
      record(false, `本地签名可解析（${name}）`, error.message);
    }
  }
  const manifestPath = path.join(projectRoot, "..", "release", "latest.json");
  if (!fs.existsSync(manifestPath)) {
    console.log("release/latest.json 不存在（尚未生成），跳过本地清单校验。");
    return;
  }
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  const problems = validateUpdaterManifest(manifest, { expectedVersion: config.version });
  record(problems.length === 0, "本地 latest.json 结构合法", problems.join("；") || manifestPath);
}

function verifySigningKeyMatchesPubkey(keyId) {
  const keyPath = (process.env.TAURI_SIGNING_PRIVATE_KEY_PATH || "").trim();
  const keyValue = (process.env.TAURI_SIGNING_PRIVATE_KEY || "").trim();
  if (!keyPath && !keyValue) {
    console.log("未提供签名私钥，跳过「私钥 ↔ 公钥一致性」校验（这是最值得先做的一步）。");
    return;
  }
  const probe = path.join(os.tmpdir(), `rlinks_probe_${Date.now()}.txt`);
  fs.writeFileSync(probe, "probe");
  const args = [path.join(projectRoot, "node_modules", "@tauri-apps", "cli", "tauri.js"), "signer", "sign"];
  if (keyPath) args.push("-f", keyPath);
  else args.push("-k", keyValue);
  args.push("-p", process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "");
  args.push(probe);
  const result = spawnSync(process.execPath, args, { encoding: "utf8" });
  fs.rmSync(probe, { force: true });
  if (result.status !== 0) {
    record(false, "私钥 ↔ 公钥一致性", `${(result.stdout || result.stderr || "").trim().slice(0, 200)}`);
    return;
  }
  try {
    const sigKeyId = minisignKeyId(fs.readFileSync(`${probe}.sig`, "utf8"), "探针签名");
    fs.rmSync(`${probe}.sig`, { force: true });
    record(
      !keyId || sigKeyId === keyId,
      "私钥 ↔ 公钥一致性",
      `签名 key id ${sigKeyId}${keyId ? ` / 公钥 ${keyId}` : ""}${sigKeyId === keyId ? "" : "（不匹配：发布后用户端必然验签失败）"}`,
    );
  } catch (error) {
    record(false, "私钥 ↔ 公钥一致性", error.message);
  }
}

/** 语义化版本比较：仅比较数字段，够用且不引入依赖。 */
function compareVersions(left, right) {
  const a = String(left).split(".").map((part) => Number.parseInt(part, 10) || 0);
  const b = String(right).split(".").map((part) => Number.parseInt(part, 10) || 0);
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    const diff = (a[index] ?? 0) - (b[index] ?? 0);
    if (diff !== 0) return diff > 0 ? 1 : -1;
  }
  return 0;
}

async function verifyRemote(config) {
  for (const endpoint of config.endpoints) {
    let response;
    try {
      response = await fetch(endpoint, { redirect: "follow" });
    } catch (error) {
      record(preRelease, "更新端点可达", `${endpoint}：${error.message}`);
      continue;
    }
    if (!response.ok) {
      record(
        preRelease,
        "更新端点存在 latest.json",
        `${endpoint} 返回 HTTP ${response.status}；应用内检查更新只会得到「缺少 latest.json 自动更新清单」`,
      );
      continue;
    }
    record(true, "更新端点存在 latest.json", endpoint);
    let manifest;
    try {
      manifest = await response.json();
    } catch (error) {
      record(false, "远端 latest.json 可解析", error.message);
      continue;
    }
    const problems = validateUpdaterManifest(manifest, {});
    record(problems.length === 0, "远端 latest.json 结构合法", problems.join("；") || `version ${manifest.version}`);
    const newer = compareVersions(manifest.version, config.version) > 0;
    record(
      newer || preRelease,
      "远端版本高于本地",
      `远端 ${manifest.version} / 本地 ${config.version}`
        + (newer ? "" : "（应用会提示「已是最新」，用户看不到更新）"),
    );
    const platform = manifest.platforms?.["windows-x86_64"];
    if (platform?.url) {
      const head = await fetch(platform.url, { method: "HEAD", redirect: "follow" }).catch((error) => ({ ok: false, status: error.message }));
      record(head.ok === true, "清单指向的安装包可下载", `${platform.url} → ${head.status}`);
    }
  }
}

async function main() {
  const config = readTauriConfig();
  console.log(`本地版本：${config.version}${preRelease ? "（发布前校验模式）" : ""}\n`);
  const keyId = verifyConfig(config);
  verifyLocalArtifacts(config, keyId);
  verifySigningKeyMatchesPubkey(keyId);
  await verifyRemote(config);

  console.log("检查结果：");
  for (const item of results) {
    console.log(`${item.ok ? "  [通过]" : "  [失败]"} ${item.label} — ${item.detail}`);
  }
  if (failed) {
    console.log("\n结论：更新链路当前无法闭环，应用内「检查更新」不会成功。");
    process.exit(1);
  }
  console.log("\n结论：更新链路各环节均已闭环。");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
