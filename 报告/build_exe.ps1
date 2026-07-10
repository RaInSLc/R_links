$ErrorActionPreference = "Stop"

Write-Host "========================================="
Write-Host "开始打包 RLinks (mod_UI) ..."
Write-Host "========================================="

$scriptDir = $PSScriptRoot
$rootDir = (Get-Item $scriptDir).Parent.FullName
$modUiDir = Join-Path -Path $rootDir -ChildPath "mod_UI"

Set-Location -Path $modUiDir
[Environment]::CurrentDirectory = $PWD.Path

Write-Host "正在安装/检查前端依赖..."
cmd /c npm ci
if ($LASTEXITCODE -ne 0) {
    Write-Host "[错误] npm ci 失败！请检查 Node 环境或网络。" -ForegroundColor Red
    Read-Host "按回车键退出..."
    exit 1
}

Write-Host "正在编译构建 EXE ..."
cmd /c npm run tauri build
if ($LASTEXITCODE -ne 0) {
    Write-Host "[错误] tauri build 失败！请检查 Rust/Cargo 环境是否就绪。" -ForegroundColor Red
    Read-Host "按回车键退出..."
    exit 1
}

Write-Host "========================================="
Write-Host "打包成功！" -ForegroundColor Green
Write-Host "========================================="

$releaseDir = Join-Path -Path $rootDir -ChildPath "release"
if (-not (Test-Path -Path $releaseDir)) {
    New-Item -ItemType Directory -Path $releaseDir | Out-Null
}

$sourceExe = Join-Path -Path $modUiDir -ChildPath "src-tauri\target\release\mod_ui.exe"
$destExe = Join-Path -Path $releaseDir -ChildPath "RLinks_UI.exe"

Copy-Item -Path $sourceExe -Destination $destExe -Force

Write-Host ""
Write-Host "已将执行文件复制到 release 目录：" -ForegroundColor Cyan
Write-Host $destExe -ForegroundColor Yellow

Read-Host "按回车键退出..."
