import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  readText,
  writeText,
} from "@tauri-apps/plugin-clipboard-manager";
import "./App.css";

type View = "workspace" | "report" | "history" | "settings";
type Method =
  | "auto"
  | "devtools"
  | "remotes"
  | "github"
  | "base"
  | "version"
  | "biocManager"
  | "checkSystem";

interface Settings {
  proxy: string;
  githubToken: string;
  cranMirror: string;
  fullSearch: boolean;
}

interface PublicSettings {
  proxy: string;
  githubTokenConfigured: boolean;
  cranMirror: string;
  fullSearch: boolean;
}

interface SearchResult {
  package: string;
  requestedVersion: string;
  latestVersion: string;
  repository: string;
  realName: string;
  source: string;
  found: boolean;
  message: string;
}

interface SearchResponse {
  runId: number;
  results: SearchResult[];
  logs: string[];
  stopped: boolean;
}

interface SearchLogEvent {
  runId: number;
  message: string;
}

interface SearchProgressEvent {
  runId: number;
  result: SearchResult;
}

interface HistoryRecord {
  id: string;
  command: string;
  packageName: string;
  version: string;
  toolName: string;
  createdAt: string;
}

const defaultSettings: Settings = {
  proxy: "",
  githubToken: "",
  cranMirror: "https://cloud.r-project.org",
  fullSearch: false,
};

const MAX_INPUT_CHARS = 100_000;
const MAX_PACKAGE_LINES = 500;
const MAX_SEARCH_TABS = 30;
const MAX_SCRIPT_CHARS = 1_000_000;
const MAX_SEARCH_RESULTS = MAX_PACKAGE_LINES * 16;
const MAX_SEARCH_LOGS = 1_000;
const MAX_STATUS_CHARS = 512;
const MAX_RESULT_FIELD_CHARS = 2_048;
const MAX_VERSION_CHARS = 64;
const MAX_SOURCE_CHARS = 16;
const MAX_HISTORY_FIELD_CHARS = 8_000;

const methods: Array<{
  id: Method;
  title: string;
  description: string;
}> = [
  { id: "auto", title: "智能路由", description: "根据检索结果自动选择来源" },
  { id: "base", title: "CRAN", description: "install.packages" },
  { id: "biocManager", title: "Bioconductor", description: "BiocManager::install" },
  { id: "github", title: "GitHub", description: "remotes::install_github" },
  { id: "remotes", title: "远程地址", description: "remotes::install_url" },
  { id: "devtools", title: "devtools", description: "devtools::install_url" },
  { id: "version", title: "版本查询", description: "packageVersion" },
  { id: "checkSystem", title: "系统检查", description: "批量检查是否已安装" },
];

const mirrors = [
  { label: "Posit Cloud", value: "https://cloud.r-project.org" },
  { label: "清华大学", value: "https://mirrors.tuna.tsinghua.edu.cn/CRAN/" },
  { label: "中国科学技术大学", value: "https://mirrors.ustc.edu.cn/CRAN/" },
  { label: "北京外国语大学", value: "https://mirrors.bfsu.edu.cn/CRAN/" },
];

const sourceNames: Record<string, string> = {
  cran: "CRAN",
  bioc: "Bioconductor",
  biocGit: "Bioc 历史版",
  github: "GitHub",
  none: "未找到",
};

let searchRunCounter = 0;

function appendBounded<T>(items: T[], item: T, limit: number) {
  if (items.length >= limit) {
    return items;
  }
  return [...items, item];
}

function takeBounded<T>(items: T[], limit: number) {
  return items.length > limit ? items.slice(0, limit) : items;
}

function App() {
  const [view, setView] = useState<View>("workspace");
  const [input, setInput] = useState("");
  const [method, setMethod] = useState<Method>("auto");
  const [conditional, setConditional] = useState(true);
  const [installDependencies, setInstallDependencies] = useState(true);
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [script, setScript] = useState("等待输入...");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [history, setHistory] = useState<HistoryRecord[]>([]);
  const [searching, setSearching] = useState(false);
  const [status, setStatus] = useState("就绪");
  const [showToken, setShowToken] = useState(false);
  const [tokenConfigured, setTokenConfigured] = useState(false);
  const activeSearchRunId = useRef(0);

  const packageCount = useMemo(
    () => input.split(/\r?\n/).filter((line) => line.trim()).length,
    [input],
  );
  const inputTooLarge = input.length > MAX_INPUT_CHARS || packageCount > MAX_PACKAGE_LINES;
  const scriptTooLarge = script.length > MAX_SCRIPT_CHARS;
  const foundCount = results.filter((result) => result.found).length;
  const uniqueFoundCount = new Set(
    results.filter((result) => result.found).map((result) => result.package),
  ).size;
  const summaryProgress = packageCount
    ? Math.min(100, (uniqueFoundCount / packageCount) * 100)
    : 0;

  useEffect(() => {
    Promise.all([
      invoke<PublicSettings>("load_settings"),
      invoke<HistoryRecord[]>("load_history"),
    ])
      .then(([savedSettings, savedHistory]) => {
        setSettings({
          proxy: savedSettings.proxy,
          githubToken: "",
          cranMirror: savedSettings.cranMirror,
          fullSearch: savedSettings.fullSearch,
        });
        setTokenConfigured(savedSettings.githubTokenConfigured);
        setHistory(takeBounded(asArray(savedHistory).map(sanitizeHistoryRecord), 100));
      })
      .catch((error) => setStatus(`初始化失败: ${formatError(error)}`));
  }, []);

  useEffect(() => {
    const unlistenLog = listen<SearchLogEvent>("search-log", (event) => {
      if (event.payload.runId !== activeSearchRunId.current) {
        return;
      }
      setLogs((current) => appendBounded(current, safeStatusText(event.payload.message), MAX_SEARCH_LOGS));
    });
    const unlistenProgress = listen<SearchProgressEvent>(
      "search-progress",
      (event) => {
        if (event.payload.runId !== activeSearchRunId.current) {
          return;
        }
        setResults((current) =>
          appendBounded(current, sanitizeSearchResult(event.payload.result), MAX_SEARCH_RESULTS),
        );
      },
    );
    return () => {
      void unlistenLog.then((unlisten) => unlisten());
      void unlistenProgress.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      if (inputTooLarge) {
        setScript("输入超出限制，无法生成脚本。");
        return;
      }
      invoke<string>("generate_script", {
        input,
        options: {
          method,
          conditional,
          installDependencies,
          mirror: settings.cranMirror,
        },
        results,
      })
        .then(setScript)
        .catch((error) => setStatus(`生成失败: ${formatError(error)}`));
    }, 120);
    return () => window.clearTimeout(timer);
  }, [
    input,
    method,
    conditional,
    installDependencies,
    settings.cranMirror,
    results,
    inputTooLarge,
  ]);

  useEffect(() => {
    const trimmed = input.trim();
    if (!trimmed || trimmed.includes("\n")) {
      return;
    }
    const containsUrl = /^https:\/\//i.test(trimmed);
    const looksLikeRepository = !containsUrl && trimmed.split(/\s+/)[0].includes("/");
    if (containsUrl && !["devtools", "remotes"].includes(method)) {
      setMethod("remotes");
    } else if (
      looksLikeRepository &&
      ["devtools", "remotes", "base", "biocManager", "version"].includes(method)
    ) {
      setMethod("github");
    }
  }, [input, method]);

  async function startSearch() {
    if (!input.trim() || searching || inputTooLarge) {
      if (inputTooLarge) {
        setStatus(`输入超出限制：最多 ${MAX_PACKAGE_LINES} 行、${MAX_INPUT_CHARS} 个字符`);
      }
      return;
    }
    setSearching(true);
    const runId = nextSearchRunId();
    activeSearchRunId.current = runId;
    setResults([]);
    setLogs([]);
    setStatus("正在检索包来源");
    setView("report");
    try {
      const response = await invoke<SearchResponse>("start_search", {
        runId,
        input,
        settings,
      });
      if (response.runId !== activeSearchRunId.current) {
        return;
      }
      setResults(takeBounded(asArray(response.results).map(sanitizeSearchResult), MAX_SEARCH_RESULTS));
      setLogs(takeBounded(asArray(response.logs).map(safeStatusText), MAX_SEARCH_LOGS));
      setStatus(response.stopped ? "检索任务已停止" : "检索完成，脚本已自动刷新");
      if (!response.stopped) {
        setMethod("auto");
      }
    } catch (error) {
      if (runId === activeSearchRunId.current) {
        setStatus(`检索失败: ${formatError(error)}`);
      }
    } finally {
      if (runId === activeSearchRunId.current) {
        setSearching(false);
        activeSearchRunId.current = 0;
      }
    }
  }

  async function stopSearch() {
    try {
      const runId = activeSearchRunId.current;
      if (!runId) {
        return;
      }
      await invoke("stop_search", { runId });
      setStatus("正在停止检索任务");
    } catch (error) {
      setStatus(`停止失败: ${formatError(error)}`);
    }
  }

  async function copyScript() {
    if (!script || script === "等待输入...") {
      return;
    }
    if (scriptTooLarge) {
      setStatus(`脚本内容过长，最多允许 ${MAX_SCRIPT_CHARS} 个字符`);
      return;
    }
    try {
      await writeText(script);
      const records = await invoke<HistoryRecord[]>("build_history_records", {
        script,
      });
      const cleanRecords = records.map(sanitizeHistoryRecord);
      const commands = new Set(cleanRecords.map((record) => record.command));
      const merged = [
        ...cleanRecords,
        ...history.filter((record) => !commands.has(record.command)),
      ].slice(0, 100);
      await invoke("save_history", { history: merged });
      setHistory(merged);
      setStatus(`已复制脚本并记录 ${records.length} 条命令`);
    } catch (error) {
      setStatus(`复制失败: ${formatError(error)}`);
    }
  }

  async function pasteInput() {
    try {
      const value = await readText();
      if (value) {
        if (value.length > MAX_INPUT_CHARS) {
          setStatus(`剪贴板内容过长，最多允许 ${MAX_INPUT_CHARS} 个字符`);
          return;
        }
        setInput(value);
        setStatus("已从剪贴板粘贴");
      }
    } catch (error) {
      setStatus(`粘贴失败: ${formatError(error)}`);
    }
  }

  async function cleanComments() {
    if (scriptTooLarge) {
      setStatus(`脚本内容过长，最多允许 ${MAX_SCRIPT_CHARS} 个字符`);
      return;
    }
    try {
      const cleaned = await invoke<string>("clean_script", { script });
      setScript(cleaned);
      setStatus("已移除脚本注释");
    } catch (error) {
      setStatus(`清理失败: ${formatError(error)}`);
    }
  }

  async function openSearchTabs() {
    const names = Array.from(
      new Set(
        input
          .split(/\r?\n/)
          .map((line) => line.trim().split(/\s+/)[0])
          .filter(Boolean)
          .map((name) => name.split("/").pop() ?? name),
      ),
    ).slice(0, MAX_SEARCH_TABS);
    if (names.length === 0) {
      setStatus("没有可搜索的包名");
      return;
    }
    let opened = 0;
    for (const name of names) {
      try {
        await invoke("open_package_search", { packageName: name });
        opened += 1;
      } catch (error) {
        setStatus(`打开搜索失败: ${formatError(error)}`);
      }
      await new Promise((resolve) => window.setTimeout(resolve, 180));
    }
    setStatus(`已打开 ${opened} 个搜索页面${packageCount > MAX_SEARCH_TABS ? `，已按上限截断到 ${MAX_SEARCH_TABS} 个` : ""}`);
  }

  async function persistSettings() {
    try {
      await invoke("save_settings", { settings });
      setTokenConfigured(settings.githubToken.trim().length > 0 || tokenConfigured);
      setSettings((current) => ({ ...current, githubToken: "" }));
      setShowToken(false);
      setStatus("设置已保存并立即生效");
    } catch (error) {
      setStatus(`设置保存失败: ${formatError(error)}`);
    }
  }

  async function clearSavedToken() {
    try {
      const publicSettings = await invoke<PublicSettings>("clear_github_token");
      setTokenConfigured(false);
      setSettings((current) => ({
        ...current,
        proxy: publicSettings.proxy,
        githubToken: "",
        cranMirror: publicSettings.cranMirror,
        fullSearch: publicSettings.fullSearch,
      }));
      setShowToken(false);
      setStatus("已清除保存的 GitHub Token");
    } catch (error) {
      setStatus(`Token 清除失败: ${formatError(error)}`);
    }
  }

  async function copyHistoryRecord(record: HistoryRecord) {
    try {
      await writeText(record.command);
      setStatus(`已复制 ${record.packageName || "历史命令"}`);
    } catch (error) {
      setStatus(`历史复制失败: ${formatError(error)}`);
    }
  }

  async function deleteHistoryRecord(id: string) {
    const next = history.filter((record) => record.id !== id).map(sanitizeHistoryRecord);
    try {
      await invoke("save_history", { history: next });
      setHistory(next);
      setStatus("历史记录已删除");
    } catch (error) {
      setStatus(`历史保存失败: ${formatError(error)}`);
    }
  }

  function isMethodDisabled(candidate: Method) {
    const trimmed = input.trim();
    if (!trimmed || trimmed.includes("\n")) {
      return false;
    }
    const containsUrl = /^https:\/\//i.test(trimmed);
    const containsSlash = trimmed.split(/\s+/)[0].includes("/");
    if (["devtools", "remotes"].includes(candidate)) {
      return !containsUrl;
    }
    if (candidate === "github") {
      return containsUrl || !containsSlash;
    }
    return containsUrl;
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">R</div>
          <div>
            <strong>Package Center</strong>
            <span>R 包命令工作台</span>
          </div>
        </div>

        <nav className="nav-list" aria-label="主导航">
          <NavButton active={view === "workspace"} label="工作台" code="01" onClick={() => setView("workspace")} />
          <NavButton active={view === "report"} label="检索报告" code="02" badge={results.length} onClick={() => setView("report")} />
          <NavButton active={view === "history"} label="命令历史" code="03" badge={history.length} onClick={() => setView("history")} />
          <NavButton active={view === "settings"} label="网络设置" code="04" onClick={() => setView("settings")} />
        </nav>

        <div className="sidebar-summary">
          <span>当前任务</span>
          <strong>{searching ? "检索中" : `${packageCount} 个输入`}</strong>
          <progress
            className="summary-track"
            value={summaryProgress}
            max={100}
            aria-label="已验证包比例"
          />
          <small>
            {results.length ? `${foundCount} 条来源记录` : "等待开始"}
          </small>
        </div>
      </aside>

      <main className="main-area">
        <header className="topbar">
          <div>
            <span className="eyebrow">R PACKAGE INSTALLATION</span>
            <h1>
              {view === "workspace" && "安装命令工作台"}
              {view === "report" && "多源检索报告"}
              {view === "history" && "命令历史"}
              {view === "settings" && "网络与镜像设置"}
            </h1>
          </div>
          <div className={`status-chip ${searching ? "active" : ""}`}>
            <i />
            {status}
          </div>
        </header>

        <section className="content">
          {view === "workspace" && (
            <div className="workspace-grid">
              <section className="panel input-panel">
                <PanelHeader
                  step="01"
                  title="输入包列表"
                  meta={`${packageCount}/${MAX_PACKAGE_LINES} 项`}
                />
                <textarea
                  value={input}
                  onChange={(event) => setInput(event.currentTarget.value)}
                  placeholder={"每行一个包，例如：\nSeurat 5.2.1\nGSVA 1.50\nbuenrostrolab/FigR\nhttps://example.org/pkg_1.0.tar.gz"}
                  spellCheck={false}
                  maxLength={MAX_INPUT_CHARS + 1}
                />
                {inputTooLarge && (
                  <div className="inline-warning">
                    输入超出限制：最多 {MAX_PACKAGE_LINES} 行、{MAX_INPUT_CHARS} 个字符。
                  </div>
                )}
                <div className="input-actions">
                  <button className="button ghost" onClick={pasteInput}>粘贴</button>
                  <button className="button ghost" onClick={() => setInput("")}>清空</button>
                  <button className="button ghost wide" onClick={openSearchTabs}>浏览器搜索</button>
                  {searching ? (
                    <button className="button danger" onClick={stopSearch}>停止</button>
                  ) : (
                    <button className="button primary" onClick={startSearch} disabled={!input.trim() || inputTooLarge}>
                      开始检索
                    </button>
                  )}
                </div>
              </section>

              <section className="panel method-panel">
                <PanelHeader step="02" title="安装策略" meta={settings.fullSearch ? "全量检索" : "快速检索"} />
                <div className="method-grid">
                  {methods.map((item) => (
                    <button
                      key={item.id}
                      className={`method-card ${method === item.id ? "selected" : ""}`}
                      disabled={isMethodDisabled(item.id)}
                      onClick={() => setMethod(item.id)}
                    >
                      <span>{item.title}</span>
                      <small>{item.description}</small>
                    </button>
                  ))}
                </div>
                <div className="toggle-row">
                  <Toggle
                    checked={conditional}
                    label="条件安装"
                    description="已安装时自动跳过"
                    onChange={setConditional}
                  />
                  <Toggle
                    checked={installDependencies}
                    label="安装依赖"
                    description="dependencies = TRUE"
                    onChange={setInstallDependencies}
                  />
                </div>
              </section>

              <section className="panel script-panel">
                <PanelHeader step="03" title="脚本预览" meta="R Script" />
                <pre>{script}</pre>
                {scriptTooLarge && (
                  <div className="inline-warning">
                    脚本内容超出限制：最多 {MAX_SCRIPT_CHARS} 个字符。
                  </div>
                )}
                <div className="script-actions">
                  <button className="button ghost" onClick={cleanComments} disabled={scriptTooLarge}>移除注释</button>
                  <button className="button primary copy-button" onClick={copyScript} disabled={!script || script === "等待输入..." || scriptTooLarge}>
                    复制完整脚本
                  </button>
                </div>
              </section>
            </div>
          )}

          {view === "report" && (
            <div className="report-layout">
              <div className="metric-row">
                <Metric label="输入包" value={packageCount} />
                <Metric label="已验证包" value={uniqueFoundCount} tone="success" />
                <Metric
                  label="未找到"
                  value={new Set(results.filter((item) => !item.found).map((item) => item.package)).size}
                  tone="danger"
                />
                <Metric label="来源记录" value={results.length} />
              </div>
              <section className="panel report-panel">
                <PanelHeader step="结果" title="来源验证" meta={searching ? "实时更新" : "已完成"} />
                {results.length === 0 ? (
                  <EmptyState text={searching ? "正在等待首条检索结果" : "尚未执行检索"} />
                ) : (
                  <div className="result-table">
                    <div className="result-row result-head">
                      <span>包名</span><span>来源</span><span>版本</span><span>仓库</span><span>状态</span>
                    </div>
                    {results.map((result, index) => (
                      <div className="result-row" key={`${result.package}-${result.source}-${index}`}>
                        <strong>{result.package}</strong>
                        <span className={`source-tag ${result.source}`}>{sourceNames[result.source] ?? result.source}</span>
                        <code>{result.latestVersion || "—"}</code>
                        <span className="repo-cell">{result.repository || "—"}</span>
                        <span className={result.found ? "found" : "missing"}>{result.found ? "已验证" : "未找到"}</span>
                      </div>
                    ))}
                  </div>
                )}
              </section>
              <section className="panel log-panel">
                <PanelHeader step="日志" title="检索过程" meta={`${logs.length} 行`} />
                <div className="log-console">
                  {logs.length ? logs.map((line, index) => <div key={`${line}-${index}`}><span>{String(index + 1).padStart(2, "0")}</span>{line}</div>) : <EmptyState text="日志将在检索开始后显示" />}
                </div>
              </section>
            </div>
          )}

          {view === "history" && (
            <section className="panel history-panel">
              <PanelHeader step="历史" title="最近生成的命令" meta={`最多保留 100 条`} />
              {history.length === 0 ? (
                <EmptyState text="复制脚本后，命令会记录在这里" />
              ) : (
                <div className="history-list">
                  {history.map((record) => (
                    <article className="history-item" key={record.id}>
                      <div className="history-main">
                        <div>
                          <strong>{record.packageName || "R 命令"}</strong>
                          <span>{record.toolName}{record.version ? ` · v${record.version}` : ""}</span>
                        </div>
                        <code>{record.command}</code>
                      </div>
                      <div className="history-actions">
                        <button className="text-button" onClick={() => copyHistoryRecord(record)}>复制</button>
                        <button className="text-button danger-text" onClick={() => deleteHistoryRecord(record.id)}>删除</button>
                      </div>
                    </article>
                  ))}
                </div>
              )}
            </section>
          )}

          {view === "settings" && (
            <div className="settings-layout">
              <section className="panel settings-panel">
                <PanelHeader step="网络" title="连接设置" meta="独立配置" />
                <label className="field">
                  <span>网络代理</span>
                  <small>支持 127.0.0.1:7890 或无凭据代理 URL，不允许路径或查询参数</small>
                  <input
                    value={settings.proxy}
                    onChange={(event) => setSettings({ ...settings, proxy: event.currentTarget.value })}
                    placeholder="不使用代理"
                  />
                </label>
                <label className="field">
                  <span>GitHub Token</span>
                  <small>
                    {tokenConfigured
                      ? "已保存 Token；留空保存会继续保留现有 Token"
                      : "仅保存在本应用的数据目录，用于提高 API 配额"}
                  </small>
                  <div className="secret-field">
                    <input
                      type={showToken ? "text" : "password"}
                      value={settings.githubToken}
                      onChange={(event) => setSettings({ ...settings, githubToken: event.currentTarget.value.trim() })}
                      placeholder="ghp_..."
                      autoComplete="off"
                      spellCheck={false}
                      maxLength={512}
                    />
                    <button type="button" onClick={() => setShowToken((value) => !value)}>
                      {showToken ? "隐藏" : "显示"}
                    </button>
                  </div>
                  {tokenConfigured && !settings.githubToken.trim() && (
                    <button type="button" className="text-button danger-text" onClick={clearSavedToken}>
                      清除已保存 Token
                    </button>
                  )}
                </label>
                <Toggle
                  checked={settings.fullSearch}
                  label="全量检索"
                  description="命中 CRAN 或 Bioconductor 后仍继续查询 GitHub"
                  onChange={(value) => setSettings({ ...settings, fullSearch: value })}
                />
              </section>

              <section className="panel settings-panel">
                <PanelHeader step="镜像" title="CRAN 镜像" meta="实时影响脚本" />
                <div className="mirror-list">
                  {mirrors.map((mirror) => (
                    <button
                      key={mirror.value}
                      className={settings.cranMirror === mirror.value ? "selected" : ""}
                      onClick={() => setSettings({ ...settings, cranMirror: mirror.value })}
                    >
                      <span>{mirror.label}</span>
                      <code>{mirror.value}</code>
                    </button>
                  ))}
                </div>
                <label className="field compact">
                  <span>自定义镜像</span>
                  <input
                    value={settings.cranMirror}
                    onChange={(event) => setSettings({ ...settings, cranMirror: event.currentTarget.value.trim() })}
                    placeholder="https://cloud.r-project.org"
                  />
                </label>
                <button className="button primary save-button" onClick={persistSettings}>保存设置</button>
              </section>
            </div>
          )}
        </section>
      </main>
    </div>
  );
}

function NavButton({
  active,
  label,
  code,
  badge,
  onClick,
}: {
  active: boolean;
  label: string;
  code: string;
  badge?: number;
  onClick: () => void;
}) {
  return (
    <button className={active ? "active" : ""} onClick={onClick}>
      <span className="nav-code">{code}</span>
      <strong>{label}</strong>
      {badge !== undefined && badge > 0 && <small>{badge}</small>}
    </button>
  );
}

function PanelHeader({
  step,
  title,
  meta,
}: {
  step: string;
  title: string;
  meta: string;
}) {
  return (
    <header className="panel-header">
      <span>{step}</span>
      <h2>{title}</h2>
      <small>{meta}</small>
    </header>
  );
}

function Toggle({
  checked,
  label,
  description,
  onChange,
}: {
  checked: boolean;
  label: string;
  description: string;
  onChange: (value: boolean) => void;
}) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.currentTarget.checked)} />
      <span className="toggle-control"><i /></span>
      <span><strong>{label}</strong><small>{description}</small></span>
    </label>
  );
}

function Metric({
  label,
  value,
  tone = "",
}: {
  label: string;
  value: number;
  tone?: string;
}) {
  return (
    <div className={`metric ${tone}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function EmptyState({ text }: { text: string }) {
  return <div className="empty-state"><span>—</span>{text}</div>;
}

function formatError(error: unknown) {
  try {
    return safeStatusText(error instanceof Error ? error.message : String(error));
  } catch {
    return "未知错误";
  }
}

function safeStatusText(value: unknown) {
  const text = String(value ?? "")
    .trim()
    .replace(/[\p{C}]/gu, "")
    .slice(0, MAX_STATUS_CHARS);
  return text || "未知错误";
}

function safeText(value: unknown, limit: number) {
  return String(value ?? "")
    .trim()
    .replace(/[\p{C}]/gu, "")
    .slice(0, limit);
}

function safeSource(value: unknown) {
  const source = safeText(value, MAX_SOURCE_CHARS);
  return Object.prototype.hasOwnProperty.call(sourceNames, source) ? source : "none";
}

function asArray<T>(value: T[] | unknown): T[] {
  return Array.isArray(value) ? value : [];
}

function sanitizeSearchResult(value: unknown): SearchResult {
  const result = value as Partial<SearchResult>;
  return {
    package: safeText(result.package, MAX_RESULT_FIELD_CHARS),
    requestedVersion: safeText(result.requestedVersion, MAX_VERSION_CHARS),
    latestVersion: safeText(result.latestVersion, MAX_VERSION_CHARS),
    repository: safeText(result.repository, MAX_RESULT_FIELD_CHARS),
    realName: safeText(result.realName, MAX_RESULT_FIELD_CHARS),
    source: safeSource(result.source),
    found: Boolean(result.found),
    message: safeStatusText(result.message),
  };
}

function sanitizeHistoryRecord(value: unknown): HistoryRecord {
  const record = value as Partial<HistoryRecord>;
  return {
    id: safeText(record.id, 64),
    command: safeText(record.command, MAX_HISTORY_FIELD_CHARS),
    packageName: safeText(record.packageName, MAX_RESULT_FIELD_CHARS),
    version: safeText(record.version, MAX_VERSION_CHARS),
    toolName: safeText(record.toolName, MAX_RESULT_FIELD_CHARS),
    createdAt: safeText(record.createdAt, 32),
  };
}

function nextSearchRunId() {
  searchRunCounter = (searchRunCounter + 1) % 1000;
  return Date.now() * 1000 + searchRunCounter;
}

export default App;
