# R_Link_Installer 功能实现记录 - 202604081520

## 实现功能
1. **基础界面构建**：基于 `ttkbootstrap` 的 `darkly` 主题。
2. **粘贴功能集成**：调用 `pyperclip.paste()` 实现一键粘贴 URL。
3. **动态代码生成**：
    - 支持 `devtools::install_url`。
    - 支持 `remotes::install_url`。
4. **复制功能集成**：调用 `pyperclip.copy()` 实现生成的代码一键复制。

## 修改详情
- 新建 `R_Link_Installer_GUI.py`。
- 新建设计文档及文档目录。
