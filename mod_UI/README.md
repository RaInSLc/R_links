# R Package Command Center

`mod_UI` 是独立的 Tauri 2 + React + Rust 桌面项目，用于生成 R 包安装命令，并检索 CRAN、Bioconductor、r-universe 与 GitHub 包来源。

旧项目 `..\cpp_src\` 只作为行为参考。本项目不会读取或修改 `cpp_src\config.ini`、`cpp_src\history.txt` 或旧版可执行文件。

## 1. 首次启动

在 Windows PowerShell 中执行：

```powershell
Set-Location -LiteralPath ".\mod_UI"
npm ci
npm run tauri dev
```

启动成功后会弹出桌面窗口。不要只运行 `npm run dev`，该命令只启动网页开发服务，不会启动 Tauri 桌面壳。

## 2. 日常开发启动

依赖已安装后，只需要：

```powershell
Set-Location -LiteralPath ".\mod_UI"
npm run tauri dev
```

如果已经把 Rust 加入系统 `PATH`，可以省略 `$env:PATH = ...` 这一行。

## 3. 构建安装包或可执行文件

```powershell
Set-Location -LiteralPath ".\mod_UI"
npm run tauri build
```

只验证集成构建、不打包安装器：

```powershell
npm run tauri build -- --no-bundle
```

构建产物位于：

```text
.\mod_UI\src-tauri\target\release\
```

项目根目录的 `报告\build_exe.bat` 是 Windows 本地打包入口，会调用 `报告\build_exe.ps1`，执行 `npm ci` 和 `npm run tauri build` 后，将便携版 `mod_ui.exe` 复制为 `release\RLinks_UI.exe`。

`release\` 是本地发布产物目录，默认不进入 Git。用于 GitHub Releases 的 `latest.json` 必须由真实构建产物和 Tauri 签名流程生成，`signature` 不能保留占位值，清单中的下载 URL 必须能下载到同版本安装包或可执行文件。

## 4. 必要环境

| 工具 | 用途 | 检查命令 |
|------|------|----------|
| Node.js | 前端依赖与 Vite 构建 | `node --version` |
| npm | 依赖安装与脚本运行 | `npm --version` |
| Rust / Cargo | Tauri 后端编译 | `cargo --version` |
| WebView2 Runtime | Windows 桌面 WebView | 系统通常已内置 |

当前机器如果 `cargo` 不在 `PATH` 中，请先将 Rust 安装目录加入当前终端的 `PATH`。默认 rustup 安装路径通常为：

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
```

## 5. 验证命令

提交前建议执行：

```powershell
Set-Location -LiteralPath ".\mod_UI"
npm run build
cargo test --manifest-path .\src-tauri\Cargo.toml --locked
cargo clippy --manifest-path .\src-tauri\Cargo.toml --all-targets --locked -- -D warnings
npm run tauri build -- --no-bundle
```

## 6. 常见问题

### `cargo` 或 `rustc` 不是命令

Rust 已安装但当前终端没有加载路径。执行：

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
```

然后重新运行 `cargo --version`。

### 在网络盘上 Vite 构建报路径冲突

Windows 映射盘与 UNC 路径混用时，Vite/Rollup 可能把同一文件解析成两个绝对路径。优先使用同一种路径启动，例如始终使用：

```powershell
Set-Location -LiteralPath ".\mod_UI"
```

不要在同一个终端中来回切换映射盘路径与 UNC 网络路径，例如 `Z:\R_links` 与 `\\10.0.0.163\pythonProject\R_links`。

### `npm ci` 很慢或失败

先确认网络可访问 npm registry。不要手工删除 `package-lock.json`，它用于锁定依赖版本。必要时可以清理本机 npm 缓存后重试：

```powershell
npm cache verify
npm ci
```

### GitHub 搜索经常限流

在界面 `网络设置` 中填写 GitHub Token。Token 只保存到 Tauri 应用数据目录，不写入源码目录；Windows 下会使用 DPAPI 加密后写入 `settings.json`。

## 7. 数据位置

应用配置与历史记录保存在 Tauri 应用数据目录中，文件名为：

```text
settings.json
history.json
```

当 JSON 损坏时，应用会自动备份为 `*.corrupt.<timestamp>.bak` 并回退到默认配置，避免启动失败。
GitHub Token 在磁盘中保存为 `githubTokenProtected` 字段，旧版明文 `githubToken` 配置仍可读取，并会在下次保存时迁移为加密字段。

## 8. 安全边界

- 前端只保留剪贴板读写权限。
- 外部浏览器搜索由 Rust 命令生成固定 `https://www.google.com/search?...` URL，前端不能打开任意 URL。
- 生产环境启用 CSP，禁止远程脚本、对象、iframe 和表单提交。
- CRAN 镜像、安装 URL、代理、GitHub 仓库名和输入规模均由 Rust 后端二次校验。
- CRAN 镜像必须使用无凭据、无查询参数或片段的 HTTPS 目录 URL；`install_url` 仅接受无凭据、无查询参数或片段的 HTTPS R 包归档 URL（`.tar.gz`、`.tar.bz2`、`.tar.xz`、`.tgz`、`.zip`）。
- 网络代理只允许无凭据的 http、https、socks5 或 socks5h 主机端口形式，不允许用户名、密码、路径、查询参数或片段。
- GitHub Token 不经 `load_settings` 回传前端，磁盘持久化使用 Windows DPAPI 保护。
- `node_modules\`、`dist\`、`src-tauri\target\` 不应提交。
