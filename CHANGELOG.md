# CHANGELOG

## [2026-06-06]

### Added
- **新增一键 Chrome 搜索功能**：
  - 在 `resources\resource.h` 中新增按钮 ID `IDC_BTN_CHROME_SEARCH` (222)。
  - 在 `app_window.hpp` 中包含 `<shellapi.h>`。
  - 在 `app_window.cpp` 中新增 “Chrome 搜索” 按钮（IDC_BTN_CHROME_SEARCH），支持提取输入框内的包名，去重后在 Chrome 中一键搜索该包名及 `R package + 包名`。当打开标签页较多（超过10个）时提供防卡死弹窗二次确认。

### Fixed
- **GitHub 模糊匹配精度修复**：
  - 在 `app_window.cpp` 的 `UpdateGeneratedCommand` 函数中，重构了同源（主要是 GitHub）下的最佳匹配选择逻辑，增加 `isExactGithubRepo` 完全相等性判定。当包名与 GitHub 仓库名（不区分大小写）完全相等的仓库存在时，优先匹配该精确仓库（如 `data2intelligence/SpaCET`），不再被 Star 数更多但仅包含包名关键字的候选仓库（如 `edzer/spacetime`）干扰。

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
