# R Package Command Center

独立的 Tauri 2 + React + Rust 桌面项目，用于生成 R 包安装命令并检索
CRAN、Bioconductor、r-universe 与 GitHub 包来源。

## 开发命令

```powershell
npm install
npm run tauri dev
```

## 构建命令

```powershell
npm run build
npm run tauri build
```

应用配置和历史记录写入 Tauri 应用数据目录，不读取或修改旧版
`cpp_src\config.ini` 与 `cpp_src\history.txt`。
