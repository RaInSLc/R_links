# R 包安装链接生成器 C++ (Win32 API) 重构记录

**时间**：2026-04-09
**版本**：v1.1 (C++ Native)
**环境**：MinGW-w64 (GCC 15.2.0)

## 1. 重构动机与收益
将原有的 Python + Tkinter 实现转换为原生 C++ 实现，主要达成以下目标：
- **零运行环境依赖**：不再需要安装 Python，生成单文件 .exe。
- **内存优化**：典型运行内存占用小于 5MB（Python 版本约为 40-60MB）。
- **启动提速**：原生代码，启动时间几乎不可感官察觉。
- **现代化视觉**：在 Win32 基础上引入了暗模式和微软雅黑字体支持。

## 2. 核心技术实现

### 2.1 模块化架构
- `PkgLogic` 类：负责正则表达式解析。
- `AppWindow` 类：封装了窗口注册、消息处理及控件自绘。
- `ClipboardUtil` 静态类：处理 Unicode 数据与系统剪贴板的交互。

### 2.2 现代化 UI 策略
- **DWM API**：通过 `dwmapi.lib` 调用 `DwmSetWindowAttribute` 实现了标题栏暗模式响应。
- **Common Controls 6.0**：通过 .manifest 清单文件启用了 Windows 自带的 XP/Vista 以后的新版视觉风格。
- **Unicode 支持**：全量采用 `wchar_t` 和 `W` 系列 API，确保中文字符串无乱码。

## 3. 源码说明
项目目录位于 `./cpp_src/`：
- `main.cpp`: 入口点。
- `app_window.hpp/cpp`: GUI 主界面逻辑。
- `pkg_logic.hpp/cpp`: 解析逻辑。
- `resources/`: 资源清单及图标脚本。
- `build.bat`: 自动化编译链接指令。

## 4. 编译操作
在 `./cpp_src/` 目录下运行 `build.bat`。该脚本会自动：
1. 使用 `windres` 编译资源。
2. 使用 `g++` 编译源码并链接必要的 Windows 库。
3. 生成 `RLinkInstaller.exe`。
