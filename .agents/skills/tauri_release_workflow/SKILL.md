---
name: tauri_release_workflow
description: Tauri 应用 GitHub Actions 自动化 Release 发布与 CI/CD 踩坑避坑指南。当处理 Tauri 项目 GitHub Release 工作流配置、tauri-action 构建发布失败、PowerShell 退出码异常、安装包归档与 updater 自动更新清单校验时触发。
---

# Tauri GitHub Actions Release 发布与 CI/CD 避坑指南

本 Skill 汇总了 Tauri v2 / v1 应用在 GitHub Actions（特别是 Windows 环境与 PowerShell 7）中进行自动化构建打包、Release 资产上传与 Updater 自动更新清单校验时踩过的核心坑点及标准解决方案。

---

## 一、 核心坑点与五项强制规范

### 1. `tauri-action` 必须显式配置 `releaseName`（致命项）
- **现象**：构建提示 `Couldn't find release with tag vX.Y.Z. Creating one.` 后随即报 `"releaseName" not set but required to create release.` 崩溃。
- **原因**：当 GitHub 上尚未创建对应 Tag 的 Release 实体时，`tauri-action` 尝试新建 Release 必须要求提供 Release 名称。
- **强制规范**：在 `tauri-action` 的 `with` 配置中，**必须同时提供 `tagName` 与 `releaseName`**。
  ```yaml
  with:
    projectPath: mod_UI
    tagName: ${{ env.RELEASE_TAG }}
    releaseName: "R Package Command Center ${{ env.RELEASE_TAG }}"
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
- **现象**：`tauri-action` 或 `softprops/action-gh-release` 报 `Tag or Release not found` 崩溃。
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
        mod_UI/src-tauri/target/release/bundle/nsis/*.exe
  ```

---

### 5. `latest.json` 自动更新清单校验必须优雅容错
- **现象**：`tauri-action` 提示 `Signature not found for the updater JSON. Skipping upload...`，随后 manifest 校验步骤死板报错。
- **原因**：当仓库 Secret 未配置 `TAURI_SIGNING_PRIVATE_KEY` 签名私钥时，Tauri 会自动跳过 `latest.json` 的生成。
- **强制规范**：校验步骤必须判断 `latest.json` 是否存在。若未上传，应记录 Notice 提示并跳过，而不是直接终止工作流：
  ```powershell
  if (-not $downloaded) {
    Write-Host "Notice: latest.json was not uploaded to release $tag (TAURI_SIGNING_PRIVATE_KEY repository secret may be unconfigured). Skipping updater manifest validation."
    $global:LASTEXITCODE = 0
    exit 0
  }
  ```

---

## 二、 标准 `release.yml` 代码模版

```yaml
name: Release

on:
  push:
    tags:
      - "v*.*.*"
  workflow_dispatch:

permissions:
  contents: write

jobs:
  build-windows:
    name: Build Windows release
    runs-on: windows-latest
    env:
      RELEASE_TAG: ${{ github.ref_name }}

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: mod_UI/package-lock.json

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache Rust dependencies
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: mod_UI/src-tauri -> target

      - name: Install frontend dependencies
        working-directory: mod_UI
        run: npm ci

      - name: Validate release tag
        shell: pwsh
        run: |
          if ($env:RELEASE_TAG -notmatch '^v\d+\.\d+\.\d+$') {
            throw "Release tag must match vX.Y.Z, got '$env:RELEASE_TAG'"
          }
          $packageVersion = (Get-Content -LiteralPath "mod_UI\package.json" -Raw | ConvertFrom-Json).version
          if ($env:RELEASE_TAG -ne "v$packageVersion") {
            throw "Release tag $env:RELEASE_TAG does not match package.json version v$packageVersion"
          }

      - name: Remove stale draft or existing release
        shell: pwsh
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          try {
            $null = gh release view $env:RELEASE_TAG --json isDraft 2>&1
            if ($LASTEXITCODE -eq 0) {
              Write-Host "Found existing release for $env:RELEASE_TAG. Deleting release to allow clean re-publish..."
              gh release delete $env:RELEASE_TAG --yes 2>&1 | Out-Null
            } else {
              Write-Host "No existing release found for $env:RELEASE_TAG. Proceeding with fresh release..."
            }
          } catch {
            Write-Host "No existing release found or check failed. Proceeding with fresh release..."
          }
          $global:LASTEXITCODE = 0
          exit 0

      - name: Build and upload Tauri assets
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        with:
          projectPath: mod_UI
          tagName: ${{ env.RELEASE_TAG }}
          releaseName: "R Package Command Center ${{ env.RELEASE_TAG }}"
          releaseBody: "Windows installation packages for ${{ env.RELEASE_TAG }}"
          releaseDraft: false
          prerelease: false
          includeUpdaterJson: true
          updaterJsonPreferNsis: true

      - name: Rename portable executable
        shell: pwsh
        run: |
          Copy-Item -LiteralPath "mod_UI\src-tauri\target\release\mod_ui.exe" -Destination "mod_UI\src-tauri\target\release\R_Package_Command_Center_Portable.exe" -Force

      - name: Upload additional release assets
        uses: softprops/action-gh-release@v2
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
          tag_name: ${{ env.RELEASE_TAG }}
          draft: false
          files: |
            mod_UI/src-tauri/target/release/R_Package_Command_Center_Portable.exe

      - name: Validate updater manifest
        shell: pwsh
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          $tag = $env:RELEASE_TAG
          New-Item -ItemType Directory -Path "updater-check" -Force | Out-Null

          $downloaded = $false
          for ($i = 1; $i -le 3; $i++) {
            Write-Host "Attempt $i/3: Downloading latest.json via gh release download..."
            try {
              $null = gh release download $tag --pattern "latest.json" --dir "updater-check" --clobber 2>&1
            } catch {}

            if (Test-Path -LiteralPath "updater-check\latest.json") {
              $downloaded = $true
              Write-Host "Successfully downloaded latest.json via GitHub CLI"
              break
            }

            Write-Host "Attempt $i/3: Fetching latest.json via Web Request..."
            $url = "https://github.com/$env:GITHUB_REPOSITORY/releases/download/$tag/latest.json"
            try {
              Invoke-WebRequest -Uri $url -OutFile "updater-check\latest.json" -Headers @{ "User-Agent" = "GitHubActions" } -ErrorAction SilentlyContinue
              if (Test-Path -LiteralPath "updater-check\latest.json") {
                $downloaded = $true
                Write-Host "Successfully downloaded latest.json via Web Request"
                break
              }
            } catch {
              Write-Host "Web Request failed: $_"
            }
            Start-Sleep -Seconds 3
          }

          if (-not $downloaded) {
            Write-Host "Notice: latest.json was not uploaded to release $tag (TAURI_SIGNING_PRIVATE_KEY repository secret may be unconfigured). Skipping updater manifest validation."
            $global:LASTEXITCODE = 0
            exit 0
          }

          $manifestPath = Join-Path "updater-check" "latest.json"
          $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
          if (-not $manifest.version) {
            Write-Error "latest.json is missing version"
            exit 1
          }
          $packageVersion = (Get-Content -LiteralPath "mod_UI\package.json" -Raw | ConvertFrom-Json).version
          if ($manifest.version -ne $packageVersion) {
            Write-Error "latest.json version $($manifest.version) does not match package.json version $packageVersion"
            exit 1
          }
          if (-not $manifest.platforms) {
            Write-Error "latest.json is missing platforms"
            exit 1
          }
          $platforms = $manifest.platforms.PSObject.Properties
          if ($platforms.Count -lt 1) {
            Write-Error "latest.json platforms is empty"
            exit 1
          }
          foreach ($platform in $platforms) {
            if (-not $platform.Value.url) {
              Write-Error "latest.json platform $($platform.Name) is missing url"
              exit 1
            }
            if (-not $platform.Value.signature) {
              Write-Error "latest.json platform $($platform.Name) is missing signature"
              exit 1
            }
          }
          Write-Host "latest.json updater manifest validated successfully!"
          $global:LASTEXITCODE = 0
          exit 0

      - name: Preserve Windows installers
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: windows-installers-${{ env.RELEASE_TAG }}
          if-no-files-found: warn
          path: |
            mod_UI/src-tauri/target/release/bundle/msi/*.msi
            mod_UI/src-tauri/target/release/bundle/nsis/*.exe
```

---

## 三、 诊断与排查速查表

| 排查现象 | 诊断原因 | 对应解决方案 |
| :--- | :--- | :--- |
| `"releaseName" not set but required` | `tauri-action` 新建 Release 缺少名称 | 在 `with` 中补全 `releaseName: "Title ${{ env.RELEASE_TAG }}"` |
| 输出了 `No release found` 随后报 exit 1 | `gh.exe` 退出码 `1` 残留在 `$LASTEXITCODE` 中 | 步骤末尾添加 `$global:LASTEXITCODE = 0; exit 0` |
| 报 `Tag not found` 或 `Release not found` | 清理步骤执行了 `--cleanup-tag` 删掉了 Git Tag | 移除清理命令中的 `--cleanup-tag` 参数 |
| `latest.json` 校验超时 404 报错 | Secrets 未配置私钥导致 `tauri-action` 跳过该文件 | 将校验改造为若未下载到 `latest.json` 则 Prompt 跳过并 exit 0 |
| Release 资产上传失败后安装包全丢 | 未配置 Actions Artifacts 归档 | 增加 `upload-artifact@v4` 步骤并开启 `if: always()` |
