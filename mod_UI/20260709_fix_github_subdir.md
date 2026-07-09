# 修复 GitHub 子目录包检索

- 允许 GitHub 仓库路径超过 2 个段（支持 owner/repo/subdir 格式）。
- 更新 aw.githubusercontent.com 路径校验逻辑，支持额外的子目录层级。
- 更新 DESCRIPTION 文件获取时的 URL 拼接，将分支名称插入到 repo 和 subdir 之间。
