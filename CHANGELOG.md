# CHANGELOG

## [2026-06-08]

### Added
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
