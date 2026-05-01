# 修改记录 - 修正 packageVersion 功能逻辑

**日期**: 2026-04-10
**时间**: 15:22
**类型**: 逻辑修正 / 规范对齐

## 问题背景
之前版本中，单选按钮 `packageVersion` 误将其功能实现为 `remotes::install_version`。根据 R 官方文档，`packageVersion()` 的功能应为返回已安装包的版本信息，而非执行安装指令。

## 修改内容
1.  **PkgLogic 逻辑纠正**：修改 `pkg_logic.cpp` 中的 `InstallMethod::Version` 分支。
    - 移除原本错误的 `remotes::install_version(...)` 生成代码。
    - 替换为标准的 R 函数调用：`packageVersion("包名")`。
2.  **UI 字面量对齐**：确保点击 `packageVersion` 单选按钮时，生成的预览内容与按钮标签所代表的 R 原生功能严格一致。

## 验证结果
- 重新编译生成 `RLinkInstaller_PRO.exe`。
- 测试输入 `xfun 0.47`：
  - 手动选中 **packageVersion**：预览框现在正确显示 `packageVersion("xfun")`（若开启条件模式则被相应包装）。
  - 功能已与官方规范对齐。

## 修改时间戳
2026-04-10 15:22
