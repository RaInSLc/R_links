# R 包搜索逻辑优化计划

## 问题分析
1. **搜索互斥问题**：当前代码在找到一个源后（如 Bioc）就会停止搜索后续源（如 GitHub），导致无法列出所有可用来源（如 `ClusterGVis`）。
2. **GitHub 搜索不稳定性**：
    - `r-universe` 搜索接口解析逻辑依赖于特定的 JSON 字段，可能因接口变动失效。
    - GitHub Search API 存在严格的频率限制（Rate Limit），未授权时极易触发 403 错误。
    - 验证 DESCRIPTION 文件时未尝试 `HEAD` 分支，可能导致非 master/main 分支的包验证失败。

## 改进方案

### 1. 并行化搜索源收集
- 修改 `SearchWorkerThread`，移除 `if (!found)` 块。
- 对每个包，依次检查 CRAN、Bioconductor、r-universe 和 GitHub Search。
- 每次命中都通过 `PostMessage` 发送进度更新。

### 2. 增强 GitHub 验证可靠性
- 在验证 GitHub 仓库时，优先尝试 `HEAD/DESCRIPTION`，这会自动重定向到仓库的默认分支。
- 增加对 GitHub API 返回状态码 403 的检测。如果触发频率限制，在报告中明确提示用户，而不是简单显示“未找到”。

### 3. 多源结果处理
- `HandleSearchProgress` 将继续追加报告行，因此多源结果会自然地在报告中并列显示。
- 修改 `UpdateGeneratedCommand` 的逻辑，使其在 `m_allSearchDone` 中存在多个同名包记录时，按照 **CRAN > Bioc > GitHub** 的优先级选择最终生成的安装命令。

### 4. 优化 r-universe 解析
- 针对 `r-universe` 搜索结果中可能缺失 `Version` 字段的情况，增加通过 `_user` 字段构造 GitHub 地址并进行二次验证的逻辑。

## 预期效果
- `ClusterGVis` 将在报告中同时显示 Bioc 和 GitHub 来源。
- `gwasvcf` 和 `gwasglue` 的搜索成功率将大幅提升，且在遇到 GitHub API 限制时会有明确提示。
- 生成的安装脚本将依然保持最优路径（优先使用 CRAN/Bioc）。

## 任务清单
- [ ] 修改 `app_window.cpp` 中的 `SearchWorkerThread` 逻辑。
- [ ] 修改 `app_window.cpp` 中的 `UpdateGeneratedCommand` 逻辑以支持优先级选择。
- [ ] 测试 `gwasvcf`, `gwasglue`, `ClusterGVis` 的搜索表现。
- [ ] 提交代码并更新 CHANGELOG。
