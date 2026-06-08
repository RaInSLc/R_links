# R Package Command Center

`mod_UI` 是独立的 Tauri 2 + React + Rust 桌面项目，用于生成 R 包安装命令，并检索 CRAN、Bioconductor、r-universe 与 GitHub 包来源。

旧项目 `..\cpp_src\` 只作为行为参考。本项目不会读取或修改 `cpp_src\config.ini`、`cpp_src\history.txt` 或旧版可执行文件。

## 1. 首次启动

在 Windows PowerShell 中执行：

```powershell
cd /d Z:\R_links\mod_UI
npm install
$env:PATH = "C:\Users\rainsc\.cargo\bin;$env:PATH"
npm run tauri dev
```

如果当前 PowerShell 不支持 `cd /d`，使用：

```powershell
Set-Location -LiteralPath "Z:\R_links\mod_UI"
npm install
$env:PATH = "C:\Users\rainsc\.cargo\bin;$env:PATH"
npm run tauri dev
```

启动成功后会弹出桌面窗口。不要只运行 `npm run dev`，该命令只启动网页开发服务，不会启动 Tauri 桌面壳。

## 2. 日常开发启动

依赖已安装后，只需要：

```powershell
Set-Location -LiteralPath "Z:\R_links\mod_UI"
$env:PATH = "C:\Users\rainsc\.cargo\bin;$env:PATH"
npm run tauri dev
```

如果已经把 Rust 加入系统 `PATH`，可以省略 `$env:PATH = ...` 这一行。

## 3. 构建安装包或可执行文件

```powershell
Set-Location -LiteralPath "Z:\R_links\mod_UI"
$env:PATH = "C:\Users\rainsc\.cargo\bin;$env:PATH"
npm run tauri build
```

只验证集成构建、不打包安装器：

```powershell
npm run tauri build -- --no-bundle
```

构建产物位于：

```text
Z:\R_links\mod_UI\src-tauri\target\release\
```

## 4. 必要环境

| 工具 | 用途 | 检查命令 |
|------|------|----------|
| Node.js | 前端依赖与 Vite 构建 | `node --version` |
| npm | 依赖安装与脚本运行 | `npm --version` |
| Rust / Cargo | Tauri 后端编译 | `cargo --version` |
| WebView2 Runtime | Windows 桌面 WebView | 系统通常已内置 |

当前机器如果 `cargo` 不在 `PATH` 中，先执行：

```powershell
$env:PATH = "C:\Users\rainsc\.cargo\bin;$env:PATH"
```

## 5. 验证命令

提交前建议执行：

```powershell
Set-Location -LiteralPath "Z:\R_links\mod_UI"
npm run build
cargo test --manifest-path .\src-tauri\Cargo.toml
cargo clippy --manifest-path .\src-tauri\Cargo.toml --all-targets -- -D warnings
npm run tauri build -- --no-bundle
```

## 6. 常见问题

### `cargo` 或 `rustc` 不是命令

Rust 已安装但当前终端没有加载路径。执行：

```powershell
$env:PATH = "C:\Users\rainsc\.cargo\bin;$env:PATH"
```

然后重新运行 `cargo --version`。

### 在网络盘上 Vite 构建报路径冲突

Windows 映射盘与 UNC 路径混用时，Vite/Rollup 可能把同一文件解析成两个绝对路径。优先使用同一种路径启动，例如始终使用：

```powershell
Set-Location -LiteralPath "Z:\R_links\mod_UI"
```

不要在同一个终端中来回切换 `Z:\R_links` 与 `\\10.0.0.163\pythonProject\R_links`。

### `npm install` 很慢或失败

先确认网络可访问 npm registry。不要手工删除 `package-lock.json`，它用于锁定依赖版本。必要时可以清理本机 npm 缓存后重试：

```powershell
npm cache verify
npm install
```

### GitHub 搜索经常限流

在界面 `网络设置` 中填写 GitHub Token。Token 只保存到 Tauri 应用数据目录，不写入源码目录。

## 7. 数据位置

应用配置与历史记录保存在 Tauri 应用数据目录中，文件名为：

```text
settings.json
history.json
```

当 JSON 损坏时，应用会自动备份为 `*.corrupt.<timestamp>.bak` 并回退到默认配置，避免启动失败。

## 8. 安全边界

- 前端只保留剪贴板读写权限。
- 外部浏览器搜索由 Rust 命令生成固定 `https://www.google.com/search?...` URL，前端不能打开任意 URL。
- 生产环境启用 CSP，禁止远程脚本、对象、iframe 和表单提交。
- CRAN 镜像、代理、GitHub 仓库名和输入规模均由 Rust 后端二次校验。
- `node_modules\`、`dist\`、`src-tauri\target\` 不应提交。
