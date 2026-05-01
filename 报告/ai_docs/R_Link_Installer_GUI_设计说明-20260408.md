# R_Link_Installer_GUI 设计说明

## 项目背景
用户需要一个简单的桌面工具，能够将 CRAN 存档中的 R 包链接自动转换为 R 的安装命令（支持 `devtools` 和 `remotes` 两种工具）。

## 功能概览
1. **URL 录入**：支持手动输入和“点击粘贴”。
2. **工具选择**：提供 RadioButton 选择生成的安装代码前缀，支持：
    - `devtools::install_url`
    - `remotes::install_url`
    - `install.packages` (原生方式)
3. **内容生成**：实时生成符合 R 语法的代码。
4. **一键复制**：提供复制按钮，将生成的结果存入剪贴板。

## 界面设计
- **框架**：使用 `ttkbootstrap` 的 `darkly` 主题。
- **布局**：垂直布局，间距适中，按钮采用醒目的色彩区分。

## 技术实现
- **Python 3.10+**
- **ttkbootstrap**: 提供现代化 UI 组件。
- **pyperclip**: 跨平台剪贴板操作。
- **tkinter**: 基础 GUI 框架。
