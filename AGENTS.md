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
