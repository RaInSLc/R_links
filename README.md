# R Package Command Center

R Package Command Center 是一个面向 Windows 的 R 包安装命令工作台。应用使用 Tauri 2、React 和 Rust 构建，可批量识别包名、版本、GitHub 仓库及 R 包归档地址，并从 CRAN、Bioconductor、r-universe 和 GitHub 检索来源后生成可直接执行的 R 命令。

## 主要功能

- 批量生成 R 包安装、版本查询和本机安装状态检查脚本。
- 支持 CRAN、Bioconductor、GitHub 和 HTTPS R 包归档。
- 支持智能路由，根据检索结果自动选择合适的安装来源。
- 实时展示来源验证结果、检索进度和运行日志。
- 自动保存最近 100 条命令，支持应用、复制和删除历史记录。
- 支持网络代理、GitHub Token、CRAN 镜像、界面主题和字体设置。
- 支持检查并安装 GitHub Releases 发布的新版本。

## 快速开始

### 使用已构建版本

打开发布目录中的应用程序，例如：

```text
release\RLinks_UI.exe
```

首次启动不需要额外配置。若需要检索 GitHub 或使用代理，可在左侧导航进入“网络设置”后完成配置。

### 从源码启动

环境要求：

- Node.js
- npm
- Rust 与 Cargo
- Microsoft Edge WebView2 Runtime

在 Windows PowerShell 中执行：

```powershell
Set-Location -LiteralPath "Z:\R_links\mod_UI"
npm ci
$env:PATH = "C:\Users\rainsc\.cargo\bin;$env:PATH"
npm run tauri dev
```

如果 Cargo 已加入系统 `PATH`，可以省略临时设置 `PATH` 的命令。

## 使用方法

### 1. 输入包列表

在“工作台”的输入框中每行填写一个项目。支持以下格式：

```text
Seurat
GSVA 1.50
buenrostrolab/FigR
https://example.org/src/contrib/demo_1.0.0.tar.gz
```

| 输入形式 | 示例 | 用途 |
|---|---|---|
| R 包名 | `Seurat` | 检索最新可用来源 |
| 包名与版本 | `GSVA 1.50` | 检索并生成指定版本命令 |
| GitHub 仓库 | `buenrostrolab/FigR` | 生成 GitHub 安装命令 |
| HTTPS 归档地址 | `https://example.org/demo_1.0.0.tar.gz` | 生成远程归档安装命令 |

空行和以 `#` 开头的注释行不会作为包处理。一次最多处理 500 个有效输入项。

### 2. 选择安装策略

通常使用默认的“智能路由”即可。应用会根据输入类型和来源检索结果自动生成命令。

| 策略 | 生成结果 |
|---|---|
| 智能路由 | 自动选择 CRAN、Bioconductor、GitHub 或远程归档 |
| CRAN | `install.packages()` |
| Bioconductor | `BiocManager::install()` |
| GitHub | `remotes::install_github()` |
| 远程地址 | `remotes::install_url()` |
| devtools | `devtools::install_url()` |
| 版本查询 | `packageVersion()` |
| 系统检查 | 批量检查包是否已经安装 |

部分策略会根据当前输入类型自动禁用。例如，GitHub 策略只适用于仓库形式输入，远程地址策略只适用于 HTTPS R 包归档。

### 3. 调整生成选项

- **条件安装**：已安装时跳过安装。
- **安装依赖**：在安装命令中启用依赖安装。
- **同步远程版本**：显示检索到的远程版本，并尽可能生成精确版本命令。
- **全量检索**：继续检查所有来源；关闭时会在获得可靠结果后减少后续请求。

### 4. 检索并复制脚本

1. 点击“开始检索”。
2. 在“脚本预览”中检查自动生成的 R 命令。
3. 点击“复制脚本”，再粘贴到 R Console、RStudio 或其他 R 运行环境中执行。

检索期间可以点击“停止”。如仅需生成基础命令，也可以在输入后直接复制实时生成的脚本。

## 页面说明

### 工作台

用于输入包列表、选择安装策略、控制检索并预览生成脚本。“浏览器搜索”可为输入中的合法包名打开外部搜索页面；超过 10 个页面时会要求确认，单次最多打开 30 个。

### 检索报告

显示输入数量、已验证包、未找到包和来源记录，并列出每个包的来源、版本、仓库及检索消息。下方日志可用于了解 CRAN、Bioconductor、r-universe 和 GitHub 的查询过程。

### 命令历史

保存最近生成的 100 条受支持命令：

- “应用”会把记录重新载入工作台。
- “复制”会将单条命令写入剪贴板。
- “删除”会移除对应记录。

### 网络设置

- **网络代理**：支持 `127.0.0.1:7890`，以及无用户名和密码的 HTTP、HTTPS、SOCKS5 或 SOCKS5H 地址。
- **GitHub Token**：可降低 GitHub API 匿名请求的频率限制影响。Token 不会回传到前端，Windows 下使用 DPAPI 加密保存。
- **CRAN 镜像**：可选择内置镜像，也可填写无凭据、无查询参数的 HTTPS 镜像目录。
- **界面风格**：支持商务办公蓝、墨绿林野和石墨暗灰主题。
- **字体风格**：支持现代、系统和经典字体方案。
- **应用更新**：手动检查 GitHub Releases 中发布的新版本。

修改网络或镜像配置后，需要点击“保存设置”。

## 数据保存

设置和历史记录保存在当前 Windows 用户的 Tauri 应用数据目录中：

```text
settings.json
history.json
```

配置或历史 JSON 损坏时，应用会创建 `*.corrupt.<timestamp>.bak` 备份并回退到默认值。GitHub Token 在 Windows 中以 DPAPI 保护的数据形式保存。

## 构建发布版本

```powershell
Set-Location -LiteralPath "Z:\R_links\mod_UI"
$env:PATH = "C:\Users\rainsc\.cargo\bin;$env:PATH"
npm run tauri build
```

仅验证 Release 集成构建而不生成安装器：

```powershell
npm run tauri build -- --no-bundle
```

Tauri 构建产物位于：

```text
mod_UI\src-tauri\target\release\
```

更完整的开发、验证和常见问题说明见 `mod_UI\README.md`。

## 项目结构

```text
R_links\
├─ mod_UI\                 Tauri 2 桌面应用
│  ├─ src\                 React 前端
│  └─ src-tauri\           Rust 后端与桌面配置
├─ release\                发布产物目录
├─ CHANGELOG.md            项目变更记录
└─ README.md               使用说明
```

## 常见问题

### GitHub 检索频率受限

在“网络设置”中填写 GitHub Token 并保存，然后重新开始检索。快速检索模式也能减少不必要的 GitHub API 请求。

### Cargo 命令不可用

确认 Rust 已安装，或在当前 PowerShell 会话中执行：

```powershell
$env:PATH = "C:\Users\rainsc\.cargo\bin;$env:PATH"
```

### 映射盘构建出现路径冲突

在同一个终端中统一使用 `Z:\R_links`，不要混用映射盘路径与 UNC 路径。

### 生成的命令无法直接执行

确认 R 环境中已安装命令所依赖的管理包，例如 `BiocManager`、`remotes` 或 `devtools`；同时检查检索报告中的来源和版本信息。
