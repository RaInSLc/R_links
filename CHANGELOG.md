# CHANGELOG

## [2026-06-09]

### Added
- **[2026-06-09 02:15:17 +08:00] `mod_UI\` 搜索日志事件边界加固**：
  - 搜索后端统一清洗 `search-log` 事件消息，移除控制字符、限制单条日志长度，并为空日志提供固定兜底文本。
  - 单次检索日志数组增加数量上限，超出后以固定截断提示占用最后一条，避免外部错误文本或高频状态日志放大前端状态。
  - 新增日志消息清洗与日志数量上限单元测试。

- **[2026-06-09 02:03:54 +08:00] `mod_UI\` 检索进度事件结果字段清洗加固**：
  - 搜索后端在发出 `search-progress` 事件前统一清洗 `SearchResult` 字段，约束包名、版本、仓库、真实名、来源和消息内容。
  - `found_result` 生成路径同步执行来源感知的仓库字段清洗，保留 `biocGit` 的 Bioconductor 版本字段，拒绝不可信 GitHub 仓库 URL。
  - 新增进度事件结果清洗与 `biocGit` 仓库版本保留单元测试。

- **[2026-06-09 01:53:32 +08:00] `mod_UI\` 浏览器搜索错误响应边界加固**：
  - `open_package_search` 的包名校验改为固定错误响应，不再把非法 `packageName` 原值拼入 IPC 错误文本，避免超长输入放大响应体。
  - 抽出浏览器搜索 URL 构造 helper，统一执行包名校验、Google 搜索 URL 构造和 URL 白名单校验。
  - 新增非法包名不回显与合法包名 URL 编码单元测试。

- **[2026-06-09 01:45:44 +08:00] `mod_UI\` 版本字段长度边界加固**：
  - R 包输入解析对请求版本号复用 64 字符上限与安全字符校验，拒绝合法字符但异常超长的版本输入。
  - 检索结果版本清洗改用同一版本边界常量，避免超长版本号进入自动安装路由、脚本预览和条件安装比较。
  - 新增超长请求版本与超长检索结果版本单元测试。

- **[2026-06-09 01:35:43 +08:00] `mod_UI\` 历史保存与 GitHub Token 发送边界加固**：
  - `save_history` 后端新增原始历史记录数量上限，直接 IPC 传入异常大数组时在清洗和写盘前拒绝。
  - GitHub Token 发送判定收紧为仅允许 `https://api.github.com`，防止未来调用点误用 HTTP GitHub API URL 时携带凭据。
  - 新增历史保存数量边界和 HTTP GitHub API 不附加 Token 单元测试。

- **[2026-06-09 01:27:03 +08:00] `mod_UI\` 脚本生成 IPC 参数边界加固**：
  - `generate_script` 后端对安装方式执行白名单与长度校验，非法 method 返回固定错误，不再把攻击性原值拼入错误响应。
  - 对前端传入的检索结果集合增加数量上限，并对结果字段长度进行预筛，避免绕过 UI 时用海量或超大结果放大脚本生成的 CPU、内存和响应体成本。
  - 新增非法 method、超量 results、超大结果字段三类单元测试。

- **[2026-06-09 01:16:36 +08:00] `mod_UI\` 前端 CSP 内联样式兼容加固**：
  - 移除侧边栏进度条的 React 内联 `style` 属性，改用原生 `progress` 元素表达验证进度。
  - 补齐 `progress` 在 WebKit 与 Gecko 下的样式，生产 CSP 无需放宽 `style-src` 即可保持界面进度反馈。

- **[2026-06-09 01:04:19 +08:00] `mod_UI\` 检索停止命令任务绑定加固**：
  - `SearchState` 记录当前运行任务 ID，`stop_search` 必须携带并匹配当前 `runId` 才能取消任务，避免旧前端回调或延迟 IPC 停止新任务。
  - `start_search` 在状态机入口拒绝 0 任务 ID，任务释放时同步清空运行 ID 和取消标志。
  - 前端停止按钮发送当前 active runId，新增陈旧停止请求拒绝与 0 runId 拒绝单元测试。

- **[2026-06-09 00:52:42 +08:00] `mod_UI\` 脚本生成输出上限加固**：
  - `generate_script` 在 Rust 后端生成完成后立即复用脚本大小上限校验，超限时直接返回错误，不再把超大脚本经 IPC 回传前端渲染。
  - 新增超大生成脚本拒绝单元测试，覆盖合法输入规模内由长镜像地址和批量条件安装放大的输出场景。

- **[2026-06-09 00:41:06 +08:00] `mod_UI\` 请求预算耗尽停止语义加固**：
  - 单次检索 HTTP 请求预算耗尽后标记任务停止，后续包和来源不再继续执行，最终响应 `stopped` 会反映该状态。
  - 预算耗尽日志去重，避免达到上限后在同一任务中重复刷出相同停止原因。
  - 新增预算耗尽停止判定单元测试，覆盖用户取消与请求预算耗尽两类停止来源。

- **[2026-06-09 00:19:41 +08:00] `mod_UI\` 单次检索 HTTP 请求预算加固**：
  - 搜索后端新增单次任务 HTTP 请求预算，所有 CRAN、Bioconductor、r-universe、GitHub API 与 DESCRIPTION 请求共享上限，避免大量输入或全量检索放大外部请求数。
  - 请求预算在实际发起网络请求前扣减，超过上限时写入搜索日志并停止后续来源请求，保留已有取消和响应大小限制。
  - 新增请求预算耗尽单元测试，覆盖达到上限后的拒绝行为。

- **[2026-06-09 00:10:12 +08:00] `mod_UI\` 外部浏览器打开限流加固**：
  - `open_package_search` 后端新增 60 秒窗口内最多 30 次打开限制，前端被绕过时仍由 Rust 强制约束外部浏览器调用频率。
  - 限流在 URL 白名单校验通过后、调用系统默认浏览器前执行，避免无效包名消耗配额。
  - 新增限流窗口边界单元测试，覆盖达到上限拒绝和窗口过期后恢复。

## [2026-06-08]

### Added
- **[2026-06-08 23:58:42 +08:00] `mod_UI\` 历史记录持久化白名单加固**：
  - `save_history` 后端不再信任前端传回的历史元数据，保存前基于命令白名单重新派生包名、版本、工具名和安全时间字段。
  - 历史记录仅允许本应用生成的 R 包安装或版本查询命令，拒绝任意命令、动态表达式、HTTP 包源和不符合模板的持久化输入。
  - `build_history_records` 与存储清洗复用同一命令识别逻辑，条件安装脚本仍能保留内部安装命令历史。

- **[2026-06-08 23:38:55 +08:00] `mod_UI\` 代理配置凭据隔离加固**：
  - 后端代理校验拒绝包含用户名、密码、路径、查询参数或片段的代理 URL，仅保留无凭据主机端口形式。
  - 设置文件损坏备份新增 `proxy` 字段脱敏，避免旧配置中的代理凭据进入 `.corrupt.*.bak` 文件。
  - 同步更新设置页提示与 README 安全边界说明，新增代理凭据拒绝与脱敏单元测试。

- **[2026-06-08 23:25:32 +08:00] `mod_UI\` Tauri IPC 权限面最小化加固**：
  - 移除主窗口 capability 中的 `core:default` 默认权限集合，避免额外暴露 path、window、webview、app、image、resources、menu、tray 等未使用核心命令。
  - 仅保留前端实际需要的事件监听、事件取消监听和文本剪贴板读写权限，后端自定义 commands 与 Rust 侧事件发送保持不变。

- **[2026-06-08 23:16:36 +08:00] `mod_UI\` 检索运行时设置合并加固**：
  - `start_search` 在后端合并已保存设置，当前端不回传 GitHub Token 时仍保留已加密保存的 Token 用于 GitHub API 检索。
  - 继续保持 `load_settings` 只返回公开设置视图，避免 Token 明文经 IPC 回流到前端。
  - 新增空 Token 继承已保存 Token、显式 Token 覆盖旧 Token 单元测试，Rust 测试总数提升至 37 项。

- **[2026-06-08 22:59:09 +08:00] `mod_UI\` 检索事件任务隔离加固**：
  - `start_search` 增加非零 `runId`，后端搜索日志、进度事件与最终响应统一携带任务 ID。
  - 前端监听 `search-log` 与 `search-progress` 时只接收当前任务 ID，任务结束后清空活动 ID，避免延迟事件污染已收敛的检索结果。
  - 新增搜索事件序列化单元测试，Rust 测试总数提升至 35 项。

- **[2026-06-08 22:43:51 +08:00] `mod_UI\` 脚本生成 IPC 结果边界加固**：
  - `generate_script` 对前端传回的 `SearchResult` 执行后端归一化，限制来源枚举、包名、版本、仓库、真实包名和消息字段。
  - 自动路由仅使用经过清洗的检索结果，非法 GitHub 仓库、非法版本或未知来源会被忽略并回退基础安装路径。
  - 新增非法检索结果拒绝、合法 GitHub 结果归一化、控制字符版本拒绝单元测试，Rust 测试总数提升至 34 项。

- **[2026-06-08 22:32:22 +08:00] `mod_UI\` 持久化并发写入加固**：
  - 为设置和历史持久化写入增加进程内互斥锁，避免多个 IPC 写入同时移动同一目标文件。
  - 原子写入改为每次使用唯一临时文件名，损坏备份文件名升级为纳秒时间戳加计数器，避免固定 `.tmp` 或 `.corrupt.*.bak` 名称冲突。
  - 保存设置时若已有设置文件损坏或超限则拒绝覆盖，避免空 Token 输入在读取失败时静默清空已保存凭据。
  - 新增唯一文件后缀与原子写入临时文件清理单元测试，Rust 测试总数提升至 31 项。

- **[2026-06-08 22:21:08 +08:00] `mod_UI\` GitHub 仓库与版本元数据边界加固**：
  - 新增 GitHub 仓库归一化逻辑，严格校验 HTTPS、`github.com` 主机、两段式 `owner/repo` 路径，拒绝查询参数、片段、额外路径和伪造 host 字符串。
  - r-universe `RemoteUrl`、显式 GitHub 输入和脚本生成统一复用仓库归一化逻辑，移除基于 `github.com/` 子串的仓库提取。
  - 对远端 HTML、DESCRIPTION 与 JSON 来源的版本字符串增加长度和字符约束，新增 2 项单元测试，Rust 测试总数提升至 29 项。

- **[2026-06-08 22:01:32 +08:00] `mod_UI\` 持久化备份与读取边界加固**：
  - 配置文件损坏备份前脱敏 `githubToken` 与 `githubTokenProtected` 字段，避免旧版明文 Token 或 DPAPI 密文进入 `.corrupt.*.bak`。
  - 为设置文件与历史文件读取增加大小上限，超限时只写入占位备份并回退默认状态，避免异常大 JSON 被一次性载入。
  - 新增正常 JSON 与 malformed JSON 损坏配置脱敏单元测试，Rust 测试总数提升至 27 项。

- **[2026-06-08 21:47:19 +08:00] `mod_UI\` 检索任务状态机加固**：
  - 将检索运行状态与取消标志收束到后端 `SearchState` 守卫中，任务结束时统一清理运行状态和取消令牌。
  - 停止检索仅对正在运行的任务生效，避免空闲状态下残留取消标志影响后续检索。
  - 新增并发启动拒绝、停止后复位、空闲停止无副作用单元测试，Rust 测试总数提升至 25 项。

- **[2026-06-08 21:35:33 +08:00] `mod_UI\` GitHub Token 磁盘加密加固**：
  - 新增 Windows DPAPI 凭据保护模块，`settings.json` 保存 `githubTokenProtected` 加密字段，不再持久化明文 `githubToken`。
  - 读取逻辑兼容旧版明文 `githubToken`，下次保存会迁移为 DPAPI 加密字段；无效或超长加密字段会触发损坏配置备份与默认回退。
  - 增加 DPAPI 加解密、旧明文兼容、加密字段不含明文、非法加密字段拒绝等单元测试，测试总数提升至 22 项。
  - 完成 `cargo test --locked`、`cargo clippy --all-targets --locked -- -D warnings`、`npm run build`、`npm run tauri build -- --no-bundle` 与危险 API / 明文 Token 扫描验证。

- **[2026-06-08 21:11:36 +08:00] `mod_UI\` GitHub Token IPC 暴露面加固**：
  - `load_settings` 改为只返回公开设置视图，前端不再通过 IPC 回读 GitHub Token 明文。
  - 保存设置时若 Token 输入为空则保留已保存 Token，保存后前端立即清空 Token 输入框并仅显示已配置状态。
  - 新增公开设置不含 Token 明文与空 Token 保留旧值单元测试，完成 `cargo test --locked`、`cargo clippy --all-targets --locked -- -D warnings` 和 `npm run tauri build -- --no-bundle` 验证。

- **[2026-06-08 21:01:22 +08:00] `mod_UI\` 包源传输安全加固**：
  - CRAN 镜像和 `devtools::install_url` / `remotes::install_url` 输入强制使用 HTTPS，拒绝明文 HTTP、带凭据或含控制字符的包源地址。
  - 前端安装 URL 自动识别同步收紧为 HTTPS，README 安全边界补充包源传输要求。
  - 新增 HTTPS 包源校验单元测试，完成 `cargo test --locked`、`cargo clippy --all-targets --locked -- -D warnings`、`npm run build` 与 `npm run tauri build -- --no-bundle` 验证。

- **[2026-06-08 20:52:32 +08:00] `mod_UI\` Rust 依赖可复现性加固**：
  - 将 `src-tauri\Cargo.toml` 直接依赖从 Cargo 默认兼容范围改为与 `Cargo.lock` 一致的精确版本。
  - README 验证命令增加 `--locked`，避免测试和 Clippy 在验证时隐式更新 Rust 依赖解析结果。
  - 通过 `cargo update --workspace --locked`、`cargo test --locked`、`cargo clippy --all-targets --locked -- -D warnings`、`npm run build` 与 `npm run tauri build -- --no-bundle` 验证。

- **[2026-06-08 20:43:02 +08:00] `mod_UI\` 前端依赖可复现性加固**：
  - 将 `package.json` 顶层 npm 依赖从 `^` / `~` 范围改为与 `package-lock.json` 一致的精确版本，降低后续安装时的供应链漂移。
  - 同步 `package-lock.json` 根依赖声明，并将 README 首次安装和故障处理命令改为 `npm ci`。
  - 通过 `npm ci --ignore-scripts --audit=false`、`npm run build`、`cargo test`、`cargo clippy --all-targets -- -D warnings`、`npm run tauri build -- --no-bundle` 和 `npm audit --audit-level=moderate` 验证。

- **[2026-06-08 20:33:35 +08:00] `mod_UI\` 安装 URL 与解析失败处理加固**：
  - 包输入解析改为对非空非注释行执行显式失败返回，避免非法输入被静默丢弃后生成误导性空脚本。
  - 对 `devtools::install_url` 与 `remotes::install_url` 的 URL 输入复用后端 URL 规范化校验，拒绝非 HTTP/HTTPS、带凭据或含控制字符的安装地址。
  - 新增 URL 安全校验单元测试，完成 `npm run build`、`cargo test`、`cargo clippy --all-targets -- -D warnings` 与 `npm run tauri build -- --no-bundle` 验证。

- **[2026-06-08 20:22:10 +08:00] `mod_UI\` Tauri 外壳安全策略收紧**：
  - 生产 CSP 移除 `style-src 'unsafe-inline'`，保持脚本、对象、iframe、表单和远程连接默认禁用。
  - 主窗口显式关闭拖放、DevTools，并启用 incognito WebView，减少非必要 WebView 状态面。
  - 增加 `Cross-Origin-Opener-Policy`、`Cross-Origin-Resource-Policy` 与 `X-Content-Type-Options` 响应头。
  - 通过 `cargo test`、`cargo clippy --all-targets -- -D warnings` 和 `npm run tauri build -- --no-bundle` 验证当前 Tauri schema 与发布构建兼容。

- **[2026-06-08 20:13:07 +08:00] `mod_UI\` 边界条件与凭据隔离加固**：
  - 为脚本清理与历史记录提取命令增加后端脚本体积上限，前端同步显示脚本超限提示并禁用高风险操作。
  - 将 HTTP 响应读取改为分块限流，避免服务端缺少 `Content-Length` 时先完整载入超大响应。
  - 将 GitHub Token 附加范围限制为 `api.github.com`，避免凭据随 r-universe 或 raw 内容请求外带。
  - 收紧历史记录持久化字段清洗，限制元数据字段长度并拒绝控制字符。
  - 调整配置与历史 JSON 写入流程，兼容 Windows 已存在目标文件时的替换语义，并保留失败回滚。
  - 新增 2 项 Rust 单元测试，完成 `npm run build`、`cargo test`、`cargo clippy --all-targets -- -D warnings`、`npm run tauri build -- --no-bundle` 和 `npm audit --audit-level=moderate` 验证。

- **[2026-06-08 19:50:41 +08:00] `mod_UI\` 工程加固与启动文档完善**：
  - 重写 `mod_UI\README.md`，补充 Windows 首次启动、日常开发、构建、验证、常见问题、数据位置和安全边界说明。
  - 收紧 Tauri 权限与 CSP：移除前端 `opener` 插件权限，生产环境禁用远程脚本、对象、iframe 和表单提交，仅保留剪贴板读写与受控 Rust 搜索命令。
  - 增加 Rust 后端输入规模限制、包名/GitHub 仓库/CRAN 镜像/代理/Token 校验、HTTP 响应大小限制和外部搜索 URL 白名单。
  - 增强配置与历史持久化容错：损坏 JSON 自动备份并回退默认值，写入采用临时文件替换，历史记录限制数量与命令长度。
  - 增强前端错误处理、输入限制提示、Token 显示切换、批量搜索上限和异步操作失败反馈。
  - Rust 单元测试增加至 11 项，并通过 `npm run build`、`cargo test`、`cargo clippy --all-targets -- -D warnings`、`npm run tauri build -- --no-bundle` 验证。

- **[2026-06-08 19:26:18 +08:00] 新增 OpenCode 依赖风险审计门 Skill**：
  - 在 `.opencode\skills\dependency-risk-audit-gate\SKILL.md` 中新增项目级技能，触发 `npm install`、`pnpm add`、`yarn add`、`pip install`、`poetry add` 等依赖安装场景。
  - 规定安装前必须输出依赖风险审计，覆盖 npm 生命周期脚本、Python 构建钩子、可疑二进制、远程执行、凭据访问和高风险 PowerShell/系统命令特征。
  - 提供允许、需要人工确认、拒绝三类判定标准，并给出 `--ignore-scripts`、`--only-binary=:all:`、`--require-hashes` 等安全替代命令。

- **[2026-06-08 19:14:30 +08:00] 新增独立 Tauri 2 桌面项目 `mod_UI\`**：
  - 使用 React 19、TypeScript、Vite、Tauri 2 与 Rust 建立独立工程，不读取、不修改 `cpp_src\` 源码及旧配置。
  - 新增工作台、检索报告、命令历史和网络设置四个功能视图，支持响应式桌面布局。
  - 在 Rust 后端重构 R 包输入解析、安装命令生成、条件安装、依赖安装、批量系统检查、注释清理与历史记录提取逻辑。
  - 新增 CRAN、Bioconductor Release、Bioconductor 历史版本、r-universe 和 GitHub 多源异步检索，支持快速/全量模式、代理、GitHub Token、任务停止与实时事件回传。
  - 新增剪贴板读写、批量浏览器搜索、CRAN 镜像选择，以及应用数据目录中的独立 JSON 配置和历史持久化。
  - 新增 6 个 Rust 单元测试，并完成 TypeScript 编译、Vite 生产构建、Clippy 严格检查和 Tauri Release 集成构建。

## [2026-06-07]

### Added
- **Bioconductor 3.23 支持**：在历史版本遍历表 `biocVersions` 中增加对最新版 `3.23` 的支持。

### Fixed
- **检索异常与停止控制流加固**：
  - 在 `SearchWorkerThread` 遇到 `InternetOpenW` 会话创建失败时，增加向主窗口投递失败消息机制，消除了界面无响应的静默卡死隐患。
  - 在工作线程因用户停止或结束退出时，向主窗口发送带业务参数的结束消息（停止为 1，初始化失败为 2，正常为 0），主窗口在收到手动停止或失败消息时只刷新状态栏，**禁止自动更新和生成预览脚本**，确保部分结果不会误包装并覆盖原始命令。
- **Bioconductor 版本推断公式纠偏**：
  - 修正了主版本 1、次版本 34 至 48 推断公式存在的 10 个次版本映射偏置 Bug（由 `(pkgMinorVersion - 34) / 2` 纠正为 `(pkgMinorVersion - 34) / 2 + 10`），使其精确指向 3.10 至 3.17，极大缩短探测冗余请求延时。
- **HTML/JSON 解析容错**：
  - 仅当网页提取到真实非空版本号时才置 `anyFound = true`，规避因网页服务正常（HTTP 200）但解析值为空时误拦截后续源回退（GitHub 检索）的控制流 Bug。

## [2026-06-06]

### Audited
- **[2026-06-06 23:55:10 +08:00] cpp_src 检索中断与逻辑完整性审核**：
  - 完成 `cpp_src\` 检索线程、WinINet 句柄、停止状态、异常路径、网络错误分类及 Bioconductor 历史检索逻辑的系统审核。
  - 确认停止路径存在同步 WinINet 调用期间跨线程关闭及重复关闭竞态，网络错误会被误判为“未找到”，手动停止会被错误报告为全量完成。
  - 审核文档保存至 `报告\ai_docs\审核\2026-06-06_审核_cpp_src检索中断与逻辑完整性.md`。
  - 使用 MinGW g++ C++17 带告警参数完成编译验证，构建成功。

### Added
- **新增一键 Chrome 搜索功能**：
  - 在 `resources\resource.h` 中新增按钮 ID `IDC_BTN_CHROME_SEARCH` (222)。
  - 在 `app_window.hpp` 中包含 `<shellapi.h>`。
  - 在 `app_window.cpp` 中新增 “Chrome 搜索” 按钮（IDC_BTN_CHROME_SEARCH），支持提取输入框内的包名，去重后在 Chrome 中一键精准搜索 `R package + 包名`。当打开标签页较多（超过10个）时提供防卡死弹窗二次确认。
  - 修复连续拉起浏览器进程导致标签页丢失的并发冲突问题。通过启用 detached 后台线程并引入 200 毫秒打开延时，确保所有搜索页面均能被稳定加载，且消除了 UI 主线程卡顿隐患。

### Fixed
- **GitHub 模糊匹配精度与大小写纠正机制修复**：
  - 在 `app_window.cpp` 的 `UpdateGeneratedCommand` 函数中，重构了同源（主要是 GitHub）下的最佳匹配选择逻辑，增加 `isExactGithubRepo` 完全相等性判定。当包名与 GitHub 仓库名（不区分大小写）完全相等的仓库存在时，优先匹配该精确仓库（如 `data2intelligence/SpaCET`），不再被 Star 数更多但仅包含包名关键字的候选仓库（如 `edzer/spacetime`）干扰。
  - 修复了因为 `r-universe` 大小写纠正导致原始大写包名在生成指令时失效的问题（如输入大写 `CARD` 却因为纠正为小写 `card` 而在主线程中字符串区分大小写比对失败，导致无法匹配并退回 CRAN 安装）。在 `SearchProgressInfo` 结构体中新增 `realName` 真实包名与 `origPkgName` 原始输入包名，并将“大小写完全严格精确匹配”作为最高优先级判别权重，使得即便 CRAN 存在模糊大小写同名包（如 `card`），工具也能正确匹配并下载到 GitHub 上严格对应大小写的唯一包（如 `YMa-lab/CARD`）。
  - 修复了大小写纠正重试机制由于错误的 `break` 语句导致直接跳出 `while(retrySearch)` 状态机重试循环并直接截断后续检索的 Bug。将 `break` 改为 `continue`，保证大小写纠正后能正确二次触发完整的重试检索。
  - 修复了 GitHub API 发生 403 限流时会针对同一个包向窗口重复投递两遍失败结果（限流提示与未找到提示）的 Bug。在 403 响应投递后将 `anyFound` 置为 `true` 阻断后续兜底投递，确保消息投递的单包原子性。
  - 增加空包名前置过滤防御。在 `SearchWorkerThread` 循环头部加入 `pkgName.empty()` 判定拦截空行，防止无效空请求消耗网络资源。

## [2026-06-03]

### C++ 原生桌面工具稳定性增强与功能优化

- **Bioconductor 版本推断公式修复**：
  - 在 `pkg_logic.hpp` 和 `pkg_logic.cpp` 中重构了 `InferBiocVersion` 联合推断规则，由仅接收次版本号改为接收主、次版本号联合推断。
  - 新增 `ParseMajorVersion` 静态方法提取主版本号。
  - 修正了例如 `GSVA 1.50 -> Bioc 3.18` 和 `GSVA 2.0 -> Bioc 3.21` 的匹配规则，并通过离线测试用例全部校验。

- **WinInet 网络句柄并发安全与超时控制**：
  - 在 `app_window.cpp` 引入了 RAII 句柄管理辅助类 `SafeRequestHandle`，确保在任何异常或分支返回时网络请求句柄都被妥善关闭。
  - 将 `m_hInternet` 和 `m_hActiveRequest` 升级为 `std::atomic<HINTERNET>` 强类型，在 UI 线程的 `IDC_BTN_STOP` 和 `WM_DESTROY` 消息处理中采用原子 `exchange` 取出并关闭句柄，彻底杜绝了并发多线程句柄竞争和双重释放风险。
  - 在 `InternetOpenW` 成功后设置了 30 秒超时选项（连接超时、发送超时、接收超时）。

- **检索模式优化（快速模式 / 全量检索开关）**：
  - 主界面新增复选框 `IDC_CHECK_FULL_SEARCH` (221)，支持勾选是否“全量检索所有源 (包括 GitHub)”。
  - 对 `ShowTabPage` 遍历控件范围扩展至 221 以兼容新控件。
  - 默认情况下（快速模式），若在 CRAN 或 Bioconductor 成功命中包信息，则跳过 GitHub API 查询，以防止大量无效查询导致 GitHub API 403 频率限制。

- **重试状态机控制流重构**：
  - 废除了检索循环中不规范的 `goto RETRY_SEARCH`，引入了清晰的 `while (retrySearch)` 状态机控制流，规范了包名大小写差异纠正重试的逻辑。

- **测试与验证脚本**：
  - 编写了非破坏性编译测试脚本 `cpp_src\build_test.bat`，用于自动化无错编译校验。
  - 在 `报告\ai_codes\bioc_infer_test.cpp` 编写了版本推断公式的离线单元测试，全部测试用例均通过验证。
