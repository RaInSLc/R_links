$ErrorActionPreference = "Stop"

Write-Host "========================================="
Write-Host "开始打包 RLinks (mod_UI) 本地免安装版 ..."
Write-Host "注：此脚本仅用于生成本地便携版，不生成自动更新清单。"
Write-Host "正式发布请使用 GitHub Actions Release 工作流。"
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

$releaseTargetDir = Join-Path -Path $modUiDir -ChildPath "src-tauri\target\release"
$sourceExe = Join-Path -Path $releaseTargetDir -ChildPath "mod_ui.exe"
if (-not (Test-Path -LiteralPath $sourceExe)) {
    $candidates = Get-ChildItem -LiteralPath $releaseTargetDir -Filter "*.exe" -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -notmatch "setup|installer|uninstall" } |
        Sort-Object LastWriteTime -Descending
    if ($candidates.Count -gt 0) {
        $sourceExe = $candidates[0].FullName
    }
}
if (-not (Test-Path -LiteralPath $sourceExe)) {
    Write-Host "[错误] 未找到 Tauri 构建产物 EXE。请检查 src-tauri\target\release 目录。" -ForegroundColor Red
    Read-Host "按回车键退出..."
    exit 1
}
$destExe = Join-Path -Path $releaseDir -ChildPath "RLinks_UI.exe"

Copy-Item -LiteralPath $sourceExe -Destination $destExe -Force

Write-Host ""
Write-Host "已将执行文件复制到 release 目录：" -ForegroundColor Cyan
Write-Host $destExe -ForegroundColor Yellow

Read-Host "按回车键退出..."
