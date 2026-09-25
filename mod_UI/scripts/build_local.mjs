import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import {
  buildUpdaterManifest,
  bundleDirs,
  collectUpdaterArtifacts,
  projectRoot,
  readTauriConfig,
  releaseAssetName,
  repoFromEndpoint,
  validateUpdaterManifest,
  verifyArtifactSignature,
} from "./updater_manifest.mjs";

const argv = process.argv.slice(2);
const hasFlag = (name) => argv.includes(name);
const flagValue = (name, fallback) => {
  const index = argv.indexOf(name);
  return index >= 0 && argv[index + 1] ? argv[index + 1] : fallback;
};

const config = readTauriConfig();
const tag = flagValue("--tag", `v${config.version}`);
const attempts = Number(flagValue("--attempts", "6"));
const releaseDir = path.resolve(projectRoot, "..", "release");
const logDir = path.resolve(projectRoot, "..", "scratch");
const logPath = path.join(logDir, "build_local.log");

/**
 * `npm run` 会从脚本环境里删掉 TEMP/TMP/USERPROFILE。MSVC 的链接器需要写
 * `%TEMP%\lnk{GUID}.tmp`，取不到就回退到对普通用户不可写的 `C:\Windows`，
 * 报 LNK1104 让整个构建失败。直接调用 CLI 时没有这个问题，但为了
 * `npm run package:local` 也能用，这里显式补齐。
 *
 * 另外实测：`tauri build` 的签名步骤**只认 `TAURI_SIGNING_PRIVATE_KEY`
 * （私钥字符串）**，给 `TAURI_SIGNING_PRIVATE_KEY_PATH` 会被忽略并报
 * 「A public key has been found, but no private key」。因此当只提供路径时，
 * 这里读文件内容补成字符串变量，避免"明明配了密钥却签不出来"。
 */
function buildChildEnv() {
  const temp = process.env.TEMP || process.env.TMP || path.join(os.homedir(), "AppData", "Local", "Temp");
  const env = { ...process.env };
  env.TEMP = temp;
  env.TMP = temp;
  env.USERPROFILE = env.USERPROFILE || os.homedir();
  env.HOMEDRIVE = env.HOMEDRIVE || path.parse(os.homedir()).root.replace(/\\$/, "");
  env.HOMEPATH = env.HOMEPATH || os.homedir().slice(env.HOMEDRIVE.length) || "\\";
  if (!env.APPDATA) env.APPDATA = path.join(os.homedir(), "AppData", "Roaming");
  if (!env.LOCALAPPDATA) env.LOCALAPPDATA = path.join(os.homedir(), "AppData", "Local");
  const keyValue = (env.TAURI_SIGNING_PRIVATE_KEY || "").trim();
  const keyPath = (env.TAURI_SIGNING_PRIVATE_KEY_PATH || "").trim();
  if (!keyValue && keyPath && fs.existsSync(keyPath)) {
    env.TAURI_SIGNING_PRIVATE_KEY = fs.readFileSync(keyPath, "utf8").trim();
    console.log(`已从 ${keyPath} 读入签名私钥（构建签名只认 TAURI_SIGNING_PRIVATE_KEY 字符串）。`);
  }
  return env;
}

function runBuild(args, env) {
  const cli = path.join(projectRoot, "node_modules", "@tauri-apps", "cli", "tauri.js");
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [cli, "build", ...args], {
      cwd: projectRoot,
      env,
      stdio: ["ignore", "pipe", "pipe"],
    });
    const chunks = [];
    const collect = (buffer) => {
      const text = buffer.toString("utf8");
      chunks.push(text);
      process.stdout.write(text);
    };
    child.stdout.on("data", collect);
    child.stderr.on("data", collect);
    child.on("close", (code) => resolve({ code, output: chunks.join("") }));
  });
}

/**
 * 网络盘（SMB）上的并发写入会间歇性返回 `os error 5（拒绝访问）`，
 * 而增量缓存让每次重试都能多编一些 crate。只对这一个特征重试，
 * 真正的编译错误立即失败，避免把错误藏起来。
 */
function isFlakyWriteFailure(output) {
  return /os error 5|拒绝访问|Access is denied|LNK1104/i.test(output);
}

async function main() {
  const hasSigningKey = Boolean(
    (process.env.TAURI_SIGNING_PRIVATE_KEY || "").trim()
    || (process.env.TAURI_SIGNING_PRIVATE_KEY_PATH || "").trim(),
  );
  if (!config.createUpdaterArtifacts) {
    console.log("警告：tauri.conf.json 未开启 bundle.createUpdaterArtifacts，不会产出 .sig 与 latest.json。");
  }
  const args = [];
  if (!hasSigningKey) {
    // 没有签名密钥时必须在本次构建关掉 updater 产物，否则 CLI 会以
    // 「A public key has been found, but no private key」直接失败。
    args.push("--config", '{"bundle":{"createUpdaterArtifacts":false}}');
    console.log(
      "提示：未检测到签名密钥（TAURI_SIGNING_PRIVATE_KEY / _PATH），本次为未签名构建，"
      + "产物不能用于自动更新；设置密钥后重跑即可产出 .sig 与 latest.json。",
    );
  } else {
    console.log("检测到签名密钥，本次构建将产出 .sig 供自动更新使用。");
  }

  if (!hasFlag("--skip-build")) {
    fs.mkdirSync(logDir, { recursive: true });
    const env = buildChildEnv();
    let succeeded = false;
    for (let attempt = 1; attempt <= attempts; attempt += 1) {
      console.log(`\n=== 构建尝试 ${attempt}/${attempts} ===`);
      const { code, output } = await runBuild(args, env);
      fs.appendFileSync(logPath, `\n===== ATTEMPT ${attempt} (exit ${code}) =====\n${output}`);
      if (code === 0) {
        succeeded = true;
        break;
      }
      if (!isFlakyWriteFailure(output)) {
        console.error("构建失败，且不是网络盘写入竞争导致的，已停止重试。");
        process.exit(1);
      }
      console.warn(`本次失败疑似网络盘写入竞争，重试。完整日志：${logPath}`);
    }
    if (!succeeded) {
      console.error(`连续 ${attempts} 次构建失败，请查看 ${logPath}`);
      process.exit(1);
    }
  }

  // 归档产物
  fs.mkdirSync(releaseDir, { recursive: true });
  const release = path.join(projectRoot, "src-tauri", "target", "release");
  const portable = path.join(release, "mod_ui.exe");
  const installerDirs = bundleDirs();
  const installerNames = [installerDirs.nsis, installerDirs.msi].flatMap((directory) => (
    fs.existsSync(directory)
      ? fs.readdirSync(directory).filter((name) => (
        name.includes(`_${config.version}_`) && (name.endsWith(".exe") || name.endsWith(".msi"))
      )).map((name) => path.join(directory, name))
      : []
  ));
  // target 目录会保留历史签名包；仅允许当前版本参与更新清单生成。
  const artifacts = hasSigningKey || hasFlag("--skip-build") ? collectUpdaterArtifacts().filter(
    (artifact) => path.basename(artifact.installer).includes(`_${config.version}_`),
  ) : [];
  for (const artifact of artifacts) verifyArtifactSignature(artifact, config.pubkey);
  if (!fs.existsSync(portable) || installerNames.length === 0) throw new Error("当前版本构建产物不完整");
  if (hasSigningKey && artifacts.length !== installerNames.length) throw new Error("当前版本签名产物不完整");
  const copied = [];
  const copy = (from, to) => {
    if (!fs.existsSync(from)) return;
    fs.copyFileSync(from, to);
    copied.push(path.basename(to));
  };
  copy(portable, path.join(releaseDir, `R_Package_Command_Center_${config.version}_portable.exe`));
  for (const installer of installerNames) {
    copy(installer, path.join(releaseDir, releaseAssetName(path.basename(installer))));
    if (!artifacts.some((artifact) => artifact.installer === installer)) {
      fs.rmSync(path.join(releaseDir, `${releaseAssetName(path.basename(installer))}.sig`), { force: true });
    }
  }
  for (const artifact of artifacts) {
    const base = path.basename(artifact.installer);
    copy(artifact.signature, path.join(releaseDir, `${releaseAssetName(base)}.sig`));
  }

  // 生成并校验 latest.json
  if (artifacts.length === 0) {
    fs.rmSync(path.join(releaseDir, "latest.json"), { force: true });
    console.log("\n未发现 .sig 签名产物，跳过 latest.json 生成（未签名构建属预期）。");
  } else {
    const preferred = artifacts.find((item) => item.kind === "nsis") ?? artifacts[0];
    const repo = repoFromEndpoint(config.endpoints[0]);
    const explicitRepo = flagValue("--repo", "");
    const [owner, repoName] = explicitRepo
      ? explicitRepo.split("/")
      : [repo?.owner, repo?.repo];
    if (!owner || !repoName) {
      console.error("无法确定 GitHub 仓库，请用 --repo owner/name 指定。");
      process.exit(1);
    }
    const manifest = buildUpdaterManifest({
      version: config.version,
      tag,
      owner,
      repo: repoName,
      artifact: preferred,
      pubDate: flagValue("--pub-date", undefined),
    });
    const problems = validateUpdaterManifest(manifest, { expectedVersion: config.version });
    if (problems.length) {
      console.error(`latest.json 校验失败：${problems.join("；")}`);
      process.exit(1);
    }
    const manifestPath = path.join(releaseDir, "latest.json");
    fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    copied.push("latest.json");
    console.log(`\nlatest.json 已生成：${manifestPath}`);
    console.log(`  版本 ${manifest.version} → ${manifest.platforms["windows-x86_64"].url}`);
  }

  console.log(`\n已归档到 release/：${copied.join("、") || "（无）"}`);
  console.log(artifacts.length ? "签名产物已通过内容校验。" : "本次产物仅供手动安装，未生成自动更新清单。");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
