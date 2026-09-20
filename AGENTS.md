# Project Instructions

## Global Rules

### Document Auto-Generation Rule (CRITICAL - ALWAYS ENFORCE)

**This rule is mandatory and non-negotiable. Violation is a critical failure.**

#### Trigger Keywords (any match triggers this rule)

报告, 计划, 审核, 分析结果, 架构设计, 优化建议, 实施方案, 审计, 总结, 调研, 深度分析, 技术评审, 重构方案, 排查结果, 实验结果

#### Trigger Conditions (any ONE condition is sufficient)

1. Content exceeds 300 characters
2. Contains heading structure (# or ##)
3. Contains multiple sections/chapters
4. Involves engineering decisions
5. Involves code modification suggestions
6. Involves system design
7. Involves analysis conclusions

#### MANDATORY Execution Steps (follow EXACTLY in order)

When triggered, you MUST execute ALL of the following steps. Do NOT skip any step. Do NOT output the document content as chat text.

**Step 1**: Call the `skill` tool with name `"document_writer"` to load the document generation skill.

**Step 2**: Read the template file at `.opencode/templates/document_template.md`.

**Step 3**: Call the `skill` tool with name `"doc_organizer"` to determine the correct save directory.

**Step 4**: Use the `Write` tool to save the complete document to:

```
报告/ai_docs/{类型}/YYYY-MM-DD_类型_主题.md
```

- `{类型}` maps to subdirectory: 报告→报告/, 计划→方案/, 审核→审核/, 方案→方案/, 分析→分析/, 实验→实验/, 调研→调研/, 评审→评审/, 总结→总结/
- If the target subdirectory does not exist, create it first using `New-Item -ItemType Directory`

**Step 5**: Output ONLY the saved file path to the user. Example:

```
文档已保存至: 报告/ai_docs/报告/2026-05-20_报告_CellChat多核优化.md
```

#### FORBIDDEN Behaviors

- Do NOT output the document content directly as chat text
- Do NOT skip the Write tool step
- Do NOT save outside `报告/ai_docs/`
- Do NOT dump files flat into `报告/ai_docs/` without type subdirectory
- Do NOT generate documents without following the template structure
- Do NOT respond with "I have generated a document" without actually calling the Write tool

### Document Output Quality

- Engineering-grade: production-ready, directly committable
- No colloquial language; use formal technical writing
- No fragmented output; always output a complete document
- Auto-complete any missing required sections (Background, Risk Assessment, Follow-up Actions)

### Language (CRITICAL - ALWAYS ENFORCE)

- 始终使用中文回复，包括对话、代码注释、文档输出。
- 总结、计划（Task / implementation_plan）及文档说明必须使用中文和 Markdown 格式。

### Windows Environment Rule

- 当前环境是 Windows 终端，所有路径必须使用反斜杠 `\`。
- 执行多条 PowerShell 命令必须使用分号 `;` 分隔，严禁使用 `&&` 操作符。

### File Safety Rule (CRITICAL - ALWAYS ENFORCE)

- 严禁修改文件编码格式，必须保持原文件编码。
- 严禁修改文件换行符，不得在 LF / CRLF 之间转换。
- 遇到乱码问题只允许修改具体字符，禁止整个文件重写。

### AI Generated Code Rule (CRITICAL - ALWAYS ENFORCE)

AI 在执行代码调整、测试、临时脚本等任务时，禁止将生成的脚本文件放到项目源码目录中。

要求：

- 所有 AI 生成的脚本、测试代码、临时代码文件，必须统一放到 `报告/ai_codes/` 目录下。
- 若 `报告/ai_codes/` 目录不存在，先创建再写入。
- 不得在项目源码目录（如 `src/`、`tests/`、项目根目录等）中随意创建临时脚本或测试文件。
- 除非用户明确指定路径，否则一律写入 `报告/ai_codes/`。

### Git Commit Rule

每完成一轮任务后，必须执行 git commit。无论操作步骤多少，只要是一轮完整任务，结束时都必须提交。

要求：

- 只提交本次任务相关文件。
- 不提交用户未要求处理的未跟踪文件或无关改动。
- 提交前运行必要验证，并在最终回复中说明验证结果。
- 提交信息使用简洁英文 Conventional Commit 风格，例如 `feat: ...`、`fix: ...`、`chore: ...`。
- 若验证无法运行或提交失败，必须在最终回复中明确说明原因。

### Project Structure Rule

- 数据库相关的修改代码必须写在 `sql/` 目录中，按功能命名。
- 所有修改内容必须在根目录 `CHANGELOG.md` 中记录，每条记录包含时间戳和功能要点。

### Security Assessment Rule

- 禁止将"可优化项"描述为"漏洞"或"安全风险"。
- 只有存在明确攻击路径、影响范围、利用条件时，才允许标记风险等级。
- 代码审查时必须区分"优化建议"与"安全问题"，不得混淆。

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tools** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them. `codegraph_node` returns one symbol's source + callers, or reads a whole file with line numbers. If the tools are listed but deferred, load them by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` and `codegraph node <symbol-or-file>` print the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->

## mod_UI 工程规则

### 目录职责

- `mod_UI/src/`：React 前端源码、组件和前端测试。
- `mod_UI/src-tauri/src/`：Tauri 命令、输入解析、搜索、脚本生成和存储逻辑。
- `mod_UI/dist/`：前端生产构建产物，由 `npm run build` 生成，不得手工修改。
- `mod_UI/src-tauri/target/`：Cargo 构建产物，禁止提交。
- `报告/ai_codes/`：AI 生成的临时脚本、诊断脚本和一次性验证代码。

### 前端验证

- 修改 `mod_UI/src/` 后运行 `npm test -- --run`。
- 修改 React、TypeScript 或 Vite 配置后运行 `npm run build`。
- 修改输入组件或输入工具函数时，必须覆盖手动输入、Enter 换行、粘贴、文件导入和受控状态回写场景。
- 受控输入组件的 `onChange` 只允许执行不会破坏编辑态的校验；去重、去空行、规范化和排序应通过显式操作或保存阶段执行。
- 多行字段不得在每次输入时调用 `trim()`、`filter(Boolean)` 或无条件删除尾随换行。
- `() => void` 形状的回调 prop 只能写成 `onClick={() => handler()}`，不得写 `onClick={handler}`。若实现实际接收可选参数（如 `useSettings` 的 `persistSettings(overrides?)`、`useAppActions` 的 `saveInputRules(rules?)`），React 会把合成事件当作该参数传入，事件对象展开后再序列化会因循环引用抛错，保存类功能会整体失效。
- 上述约定必须由测试固定：组件测试断言 `toHaveBeenCalledWith()`（零实参），且 `App.test.tsx` 保留端到端用例断言保存请求的负载可被 `JSON.stringify` 序列化、字段类型正确。
- 版本号禁止硬编码回退值：运行时取 `@tauri-apps/api/app` 的 `getVersion()`，失败时回退构建期注入的 `__APP_VERSION__`（见 `vite.config.ts` 的 `define` 与 `src/utils-update.ts`）。
- 源码规模门禁 `npm run check:size` 采用**棘轮基线**：未列入 `scripts/check_source_size.mjs` 中 `MAX_LINE_CHAR_BASELINE` 的文件（即新增模块）一行都不允许超过 200 字符；已列入的文件不得比基线更多。修完某个文件的超长行后必须把对应数字改小或删除，基线与 `eslint.config.mjs` 的 `max-len` 策略保持一致。
- 从后端加载的配置对象（`load_input_rules`、`load_settings`）必须在边界处用 `settingsSanitize.ts` 的 `sanitizeImported*` 补齐字段后再进入状态，不得让消费方假设字段完整。

### Tauri 与 Rust 验证

- 修改 `mod_UI/src-tauri/src/` 后运行 `cargo test`，必要时运行 `cargo check`。
- 输入解析、脚本生成和 URL 校验的修改必须增加对应 Rust 回归测试。
- URL 规则必须区分用途：CRAN 镜像和检索请求保持既有 HTTPS 白名单；本地 R 包归档可按需求使用 HTTP，但必须继续校验归档扩展名、主机、凭据、查询参数和片段。
- 前端对输入类型的识别必须与后端保持一致，HTTP 和 HTTPS 归档不得出现统计、去重、智能建议或自动路由不一致。

### 构建与发布

- Tauri 配置中的 `beforeBuildCommand` 会先生成 `mod_UI/dist/`，发布前必须使用完整 Tauri 构建流程验证。
- 修改前端源码后不得只运行旧安装包验证；必须重新执行前端构建并重新打包安装程序。
- `tauri.key`、Token、代理凭据和用户数据不得提交。
- 未经用户明确要求，不提交 `README.md` 等无关工作区改动。
- **打包必须绕开 npm 的环境剥离**：`npm run` 会把 `TEMP`、`TMP`、`USERPROFILE` 从脚本环境中删掉（`npm run env` 只剩约 40 个变量），MSVC `link.exe` 取不到 `%TEMP%` 时会回退到对当前用户不可写的 `C:\Windows`，报 `LNK1104: 无法打开文件 "C:\Windows\lnk{...}.tmp"`，使 `npm run tauri build` **必然失败**。正确命令是在 `mod_UI/` 下执行 `./node_modules/.bin/tauri build`（直接调用 CLI 的 node 入口，继承完整环境）。
- 本机 `Z:` 是 SMB 网络共享（`\\10.0.0.163\pythonProject`），Rust 全量编译会间歇性报 `os error 5（拒绝访问）`（并发写入竞争，已排除磁盘空间与权限）。使用 `报告/ai_codes/retry_build.sh <工作目录> <日志文件> <最大次数> -- <命令>` 做"失败即重试"，依托 cargo 增量缓存逐次推进：实测约 11~13 crate/min，明显快于 `cargo test -j 1` 的约 2.7 crate/min（后者虽稳定但代价过高）。
- 产物归档：安装程序与免安装主程序在构建后复制到根目录 `release/`（该目录已被 `.gitignore` 忽略）。
- 本地打包统一用 `npm run package:local`（等价 `node scripts/build_local.mjs`）：它会补齐被 npm 剥离的 `TEMP`/`TMP`/`USERPROFILE`，只在"网络盘写入竞争"特征出现时重试，按有无签名密钥决定是否产出 updater 产物，并在有 `.sig` 时生成并校验 `release/latest.json`。

### 自动更新链路（发布前必须逐项确认）

自动更新是「GitHub Release + minisign 签名 + latest.json + 应用内置公钥」四者缺一不可的闭环，**任何一环缺失都不会让构建或发布报错**，只会在用户端表现为一句笼统的失败提示。因此发布前必须：

- `bundle.createUpdaterArtifacts` 必须为 `true`。它是 `.sig` 与 `latest.json` 的唯一来源；历史上该字段从未配置过，导致十余个版本全部没有更新清单。
- `plugins.updater.pubkey` 必须是**合法且与签名私钥同源**的 minisign 公钥。可用 `npm run verify:updater` 校验：它会解析公钥、比对私钥签名出的 key id、检查远端 `latest.json` 与资产可达性。
- 写入公钥时禁止经过会解释转义字符的通道（PowerShell 的 `` ` `` 是转义符，曾把公钥主体破坏成含退格符的非法 base64）。**必须**通过脚本或 `tauri.key.*.pub` 文件内容直接写入，写完立刻跑 `npm run verify:updater`。
- 更新密钥对与密码：私钥文件为 `mod_UI/tauri.key.regenerated`（被 `tauri.key.*` 规则忽略，**不得提交**），对应 GitHub Secrets 为 `TAURI_SIGNING_PRIVATE_KEY`（私钥文件内容）与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`（无密码留空）。
- 版本号必须三处同步（`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`）且与 tag 一致；tag 必须严格高于用户已装版本，否则应用只会提示"已是最新"。
- 发布工作流在缺少签名密钥或缺少 `latest.json` 时**必须硬失败**，禁止再出现"打印 Notice 后 exit 0"的静默跳过。
- `mod_UI/tauri.key`（旧私钥，密码已丢失）不得再用于任何发布，仅作历史留存。

### 配置与文档

- `AGENTS.md` 是项目协作规则，必须纳入版本控制，不得在任何 `.gitignore` 中忽略。
- 每轮代码修改都必须同步根目录 `CHANGELOG.md`，记录时间戳、变更点和验证结果。
- 结构化审核、分析、总结或方案文档必须按文档技能规则保存到 `报告/ai_docs/{类型}/`，不得直接输出完整文档内容替代文件。
