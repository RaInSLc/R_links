# R_Link_Installer 功能增强记录 - 202604081524

## 新增功能
- **install.packages 支持**：在原有 `devtools` 和 `remotes` 基础上，新增了 R 原生 `install.packages` 的生成选项。

## 修改详情
- **GUI 更新**：在“安装工具选择”区域新增了一个 RadioButton。
- **逻辑更新**：
    - 选中 `install.packages` 时，生成的代码格式为：`install.packages("URL", repos = NULL, type = "source")`。
    - 保持了实时生成的特性，切换选项或更改 URL 均会自动更新结果。

## 布局优化
- 将选项按钮的间距从 `padx=10` 调整为 `padx=5`，以容纳新增的第三个按钮，确保在默认窗口宽度下不拥挤。
