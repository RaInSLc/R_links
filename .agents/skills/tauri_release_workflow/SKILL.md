---
name: tauri_release_workflow
description: Tauri 应用 GitHub Actions 自动化 Release 发布与 CI/CD 踩坑避坑指南。当处理 Tauri 项目 GitHub Release 工作流配置、tauri-action 构建发布失败、PowerShell 退出码异常、安装包归档，以及 updater 自动更新清单（latest.json）与签名校验时触发。
---

# Tauri GitHub Actions Release 发布与 CI/CD 避坑指南

本 Skill 汇总了 Tauri v2 / v1 应用在 GitHub Actions（特别是 Windows 环境与 PowerShell 7）中进行自动化构建打包、Release 资产上传与 Updater 自动更新清单校验时踩过的核心坑点及标准解决方案。

> 本 Skill 描述的是仓库当前的**现状规范**。真实的发布流水线以仓库内 `.github/workflows/release.yml` 为唯一权威来源；若本文件与它不一致，**以 `release.yml` 为准**，并回头同步修正本文件。历史上正是因为文档与流水线长期脱节、把致命错误当成"可优雅跳过"，才导致连续多个版本全部无法自动更新。

---

## 一、 核心坑点与五项强制规范

### 1. `tauri-action` 必须显式配置 `releaseName`（致命项）
- **现象**：构建提示 `Couldn't find release with tag vX.Y.Z. Creating one.` 后随即报 `"releaseName" not set but required to create release.` 崩溃。
- **原因**：当 GitHub 上尚未创建对应 Tag 的 Release 实体时，`tauri-action` 尝试新建 Release 必须要求提供 Release 名称。
- **强制规范**：在 `tauri-action` 的 `with` 配置中，**必须同时提供 `tagName` 与 `releaseName`**，且 `releaseName` 必须带版本号（现状直接复用 tag 变量）：
  ```yaml
  with:
    projectPath: mod_UI
    tagName: ${{ env.RELEASE_TAG }}
    releaseName: ${{ env.RELEASE_TAG }}
    releaseBody: "Windows installation packages for ${{ env.RELEASE_TAG }}"
  ```

---

### 2. PowerShell (`pwsh`) 步骤的外部命令退出码踩坑（致命项）
- **现象**：日志最后一行输出了正常日志（如 `No existing release found...`），但 GitHub Runner 紧接着报告 `Process completed with exit code 1.`。
- **原因**：PowerShell 7 会记录上一个调用的原生 `.exe` 命令（如 `gh release view` 或 `gh release download`）的非零退出码至 `$LASTEXITCODE`。即便在 PowerShell 中使用了 `try / catch` 或 `if / else` 正确处理了分支，如果脚本结束时未显式清零，Runner 依然会判定步骤失败。
- **强制规范**：所有调用过外部 CLI 命令的 `pwsh` 步骤，在成功分支末尾**必须显式重置退出码**：
  ```powershell
  $global:LASTEXITCODE = 0
  exit 0
  ```

---

### 3. 禁止在 Release 清理步骤中使用 `--cleanup-tag`
- **现象**：`tauri-action` 或后续上传步骤报 `Tag or Release not found` 崩溃。
- **原因**：`gh release delete --cleanup-tag` 会将 GitHub 远程仓库上的 Git Tag 直接删掉。构建脚本后置步骤依赖该 Git Tag 绑定资产时，因标签已失踪而崩溃。
- **强制规范**：清理旧 Release 时，**绝对禁止添加 `--cleanup-tag`**。
  ```powershell
  gh release delete $env:RELEASE_TAG --yes 2>&1 | Out-Null
  ```

---

### 4. 必须配置 `actions/upload-artifact@v4` 进行安装包兜底归档
- **现象**：编译成功打出了 `.msi` 或 `.exe`，但由于后续发布网络抖动或校验失败，Runner 被回收后安装包彻底丢失。
- **强制规范**：在工作流末尾必须添加 `Preserve Windows installers` 步骤，并设置 `if: always()`：
  ```yaml
  - name: Preserve Windows installers
    if: always()
    uses: actions/upload-artifact@v4
    with:
      name: windows-installers-${{ env.RELEASE_TAG }}
      if-no-files-found: warn
      path: |
        mod_UI/src-tauri/target/release/bundle/msi/*.msi
        mod_UI/src-tauri/target/release/bundle/msi/*.sig
        mod_UI/src-tauri/target/release/bundle/nsis/*.exe
        mod_UI/src-tauri/target/release/bundle/nsis/*.sig
  ```

---

### 5. 缺签名密钥 / 缺 `latest.json` 必须硬失败，绝对禁止静默跳过（致命项）
- **现象**：`tauri-action` 提示 `Signature not found for the updater JSON. Skipping upload...`，Release 里没有 `latest.json`。
- **原因**：当仓库 Secret 未配置 `TAURI_SIGNING_PRIVATE_KEY` 签名私钥时，Tauri 会静默跳过 `.sig` 与 `latest.json` 的生成；而 `latest.json` 正是应用内「检查更新」唯一读取的清单。
- **历史事故**：早期校验步骤在拿不到 `latest.json` 时只打印一行 Notice 就 `exit 0`。结果是发布流水线一路绿灯，十余个版本全部被发出去，客户端却永远拿不到更新清单——"静默跳过"把一次本可在 CI 当场发现的配置错误，变成了长期的线上隐性故障。
- **强制规范**：缺密钥或缺清单都必须**硬失败**，让问题在发布当刻暴露，绝不能"优雅容错"：
  - 发布流程开头必须新增 `Require updater signing key` 步骤：`TAURI_SIGNING_PRIVATE_KEY` 为空时直接 `throw`，拒绝发布注定无法自动更新的版本。
  - `Validate updater manifest` 步骤在下载不到 `latest.json` 时，必须 `Write-Error` + `exit 1`，绝不能打印 Notice 后 `exit 0`：
  ```powershell
  if (-not $downloaded) {
    Write-Error "latest.json was not uploaded to release $tag"
    exit 1
  }
  ```
- **为什么不能静默跳过**：发布流程不会因为缺少签名而报错，只会产出"看似正常"的 Release；用户端自动更新的失败提示又极为笼统，排查成本极高。宁可在 CI 上失败一次、要求维护者补齐密钥，也不要让线上用户长期收不到更新。

---

## 二、 标准 `release.yml` 现状（唯一权威来源）

**本仓库的发布流水线以 `.github/workflows/release.yml` 为唯一权威来源。** 本 Skill **不再内嵌可直接复制的完整模板**——历史上正是因为内嵌模板长期未同步（仍写 `softprops/action-gh-release`、缺少发布前门禁与签名密钥校验、`releaseName` 缺版本号），维护者照抄模板就退回了旧行为。需要新配置或排障时，请直接阅读并按现状修改该文件，不要凭记忆重写。

当前 `release.yml` 的 `build-windows` job（`runs-on: windows-latest`）关键步骤顺序如下，任何改动都必须保持这条链完整：

1. **`Require updater signing key`**：`TAURI_SIGNING_PRIVATE_KEY` 为空即 `throw`，拒绝发布无法自动更新的版本。
2. **`Validate release tag`**：tag 必须匹配 `vX.Y.Z`，且与 `package.json` 的版本一致。
3. **`Validate tests and build before publishing`**：发布前门禁。先校验版本号三处同步，再依次执行 `npm test -- --run`、`npm run lint`、`npm run check:size`、`cargo fmt --manifest-path src-tauri\Cargo.toml -- --check`、`cargo test --locked`、`cargo clippy --all-targets --locked -- -D warnings`、`tauri build --no-bundle`。此处只验证编译与测试；签名打包留给下一步，避免因缺少签名环境变量而失败。
4. **`Build and upload Tauri assets`**：`tauri-apps/tauri-action@v0`，配置 `releaseName: ${{ env.RELEASE_TAG }}`（带版本号）、`includeUpdaterJson: true`、`updaterJsonPreferNsis: true`，并通过 `env` 注入签名私钥与密码。
5. **`Rename portable executable` + `Upload portable release asset`**：复制出免安装主程序后，用 `gh release upload $env:RELEASE_TAG <file> --clobber` 上传（**不再使用 `softprops/action-gh-release`**）。
6. **`Validate updater manifest`**：下载并校验 `latest.json`，缺失即 **硬失败**（见第一节第 5 条）。
7. **`Verify updater chain end to end`**：调用 `node scripts/verify_updater.mjs --pre-release` 做端到端自检（含 CDN 缓存延迟重试）。
8. **`Preserve Windows installers`**（`if: always()`）：用 `actions/upload-artifact@v4` 兜底归档 `.msi` / `.exe` 及其 `.sig`。

> 禁止把上述任何一步改回「打印 Notice 后 `exit 0`」的静默跳过，也禁止把 `gh release upload` 换回 `softprops/action-gh-release`，或把 `releaseName` 改成不带版本号的固定字符串。

---

## 三、 诊断与排查速查表

| 排查现象 | 诊断原因 | 对应解决方案 |
| :--- | :--- | :--- |
| `"releaseName" not set but required` | `tauri-action` 新建 Release 缺少名称 | 在 `with` 中补全 `releaseName: ${{ env.RELEASE_TAG }}` |
| 输出了 `No release found` 随后报 exit 1 | `gh.exe` 退出码 `1` 残留在 `$LASTEXITCODE` 中 | 步骤末尾添加 `$global:LASTEXITCODE = 0; exit 0` |
| 报 `Tag not found` 或 `Release not found` | 清理步骤执行了 `--cleanup-tag` 删掉了 Git Tag | 移除清理命令中的 `--cleanup-tag` 参数 |
| `latest.json` 校验超时 404 报错 | Secrets 未配置签名私钥，或 `bundle.createUpdaterArtifacts` 非 `true`，导致 `tauri-action` 跳过了清单生成 | **这是发布失败，不是可跳过的告警**：排查 `TAURI_SIGNING_PRIVATE_KEY` 与 `bundle.createUpdaterArtifacts`，绝不允许改成"未下载到 `latest.json` 就 Prompt 跳过并 exit 0" |
| `cargo clippy` / `cargo fmt --check` 报错 | 发布前门禁未通过 | `cargo clippy --all-targets --locked -- -D warnings` 与 `cargo fmt --check` 是发布前门禁，必须在 `Validate tests and build before publishing` 中通过后才能继续 |
| Release 资产上传失败后安装包全丢 | 未配置 Actions Artifacts 归档 | 增加 `upload-artifact@v4` 步骤并开启 `if: always()` |

---

## 四、 自动更新链路的关键机制（发布前必须逐项确认）

自动更新是「GitHub Release + minisign 签名 + `latest.json` + 应用内置公钥」四者缺一不可的闭环：**任何一环缺失都不会让构建或发布报错**，只会在用户端表现为一句笼统的失败提示。以下机制真实存在于现状流水线，发布前必须逐项确认：

- **`npm run verify:updater` 端到端自检**：运行 `node scripts/verify_updater.mjs`，解析内置公钥、比对签名出的 key id、检查本地 `.sig` 产物，并拉取远端 `latest.json` 与资产可达性。加 `--pre-release` 用于发布流程内部（此时"远端版本尚未高于本地、端点暂不可达"属预期），由 `release.yml` 的 `Verify updater chain end to end` 步骤调用。
- **`bundle.createUpdaterArtifacts` 必须为 `true`**（`src-tauri/tauri.conf.json`）：它是 `.sig` 与 `latest.json` 的唯一来源；历史上该字段从未配置过，导致十余个版本全部没有更新清单。
- **`plugins.updater.pubkey` 必须与签名私钥同源且 key id 一致**：必须是合法且与私钥成对匹配的 minisign 公钥。用互不匹配的密钥发布后，用户端必然验签失败；可用 `npm run verify:updater` 校验。
- **禁止经过会解释转义字符的通道写公钥**：PowerShell 的 `` ` `` 是转义符，曾把公钥主体破坏成含退格符的非法 base64。**必须**通过脚本或 `tauri.key.*.pub` 文件内容直接写入，写完立刻跑 `npm run verify:updater`。
- **版本号三处同步**：`mod_UI/package.json`、`mod_UI/src-tauri/tauri.conf.json`、`mod_UI/src-tauri/Cargo.toml` 的版本号必须一致，且与发布 tag 一致；tag 必须严格高于用户已安装版本，否则应用只会提示"已是最新"。
