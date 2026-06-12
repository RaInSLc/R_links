@echo off
chcp 65001 >nul
echo =========================================
echo 开始打包 RLinks (mod_UI) ...
echo =========================================

cd /d "%~dp0mod_UI"

echo 正在安装/检查前端依赖...
call npm install
if %errorlevel% neq 0 (
    echo [错误] npm install 失败！请检查 Node 环境或网络。
    pause
    exit /b %errorlevel%
)

echo 正在编译构建 EXE ...
call npm run tauri build
if %errorlevel% neq 0 (
    echo [错误] tauri build 失败！请检查 Rust/Cargo 环境是否就绪。
    pause
    exit /b %errorlevel%
)

echo =========================================
echo 打包成功！
echo =========================================

if not exist "%~dp0release" mkdir "%~dp0release"
copy /y "src-tauri\target\release\mod_ui.exe" "%~dp0release\RLinks_UI.exe"

echo.
echo 已将执行文件复制到 release 目录：
echo %~dp0release\RLinks_UI.exe
pause
