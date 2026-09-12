import { describe, it, expect, vi, beforeEach } from "vitest";
import "@testing-library/jest-dom";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";
import { ReportView } from "./ReportView";
import type { SearchResult } from "./utils";
import { invoke } from "@tauri-apps/api/core";
import { defaultSettings } from "./types";
import { getInstallCommand, mergeInstallCommands } from "./reportUtils";

// Mock @tauri-apps/api/core and @tauri-apps/plugin-clipboard-manager
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(),
}));

describe("ReportView", () => {
  it("多个 CRAN 包合并且保留库路径与条件安装", () => {
    const results = ["dplyr", "ggplot2"].map((packageName) => ({ package: packageName, realName: packageName, source: "cran", found: true, requestedVersion: "", latestVersion: "1.0", repository: "", message: "" }));
    const script = mergeInstallCommands(results, { ...defaultSettings, rLibPath: "D:/R/library" });
    expect(script.match(/install\.packages\(/g)).toHaveLength(1);
    expect(script).toContain('c("dplyr", "ggplot2")');
    expect(script).toContain('Filter(function(p) !requireNamespace(p, quietly = TRUE)');
    expect(script).toContain('lib = "D:/R/library"');
    const explicit = mergeInstallCommands([{ ...results[0], requestedVersion: "1.0" }, results[1]]);
    expect(explicit).toContain('install_version("dplyr"');
  });
  beforeEach(() => {
    vi.mocked(invoke).mockImplementation(async (command, args) => command === "generate_result_commands" ? (args as { results: SearchResult[] }).results.map(getInstallCommand) : undefined);
  });
  it("报告复制复用后端命令并传递当前库路径", async () => {
    const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
    vi.mocked(invoke).mockResolvedValue(['install.packages("dplyr", lib = "D:/R/custom")']);
    const results: SearchResult[] = [{ package: "dplyr", requestedVersion: "", latestVersion: "1.1.0", repository: "", realName: "dplyr", source: "cran", found: true, message: "" }];
    render(<ReportView results={results} logs={[]} dependencyGraph={null} packageCount={1} uniqueFoundCount={1} smartSuggestions={[]} searching={false} searchDuration={1} settings={{ ...defaultSettings, rLibPath: "D:/R/custom" }} onClearLogs={vi.fn()} onStatusChange={vi.fn()} onApplySmartSuggestion={vi.fn()} onRetryMissing={vi.fn()} />);
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 180)); });
    await act(async () => { fireEvent.click(screen.getAllByTitle(/复制安装指令/)[0]); });
    expect(invoke).toHaveBeenCalledWith("generate_result_commands", expect.objectContaining({ options: expect.objectContaining({ rLibPath: "D:/R/custom" }) }));
    expect(writeText).toHaveBeenCalledWith('install.packages("dplyr", lib = "D:/R/custom")');
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "合并指令" })); });
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining('lib = "D:/R/custom"'));
    const calls = vi.mocked(writeText).mock.calls;
    expect(calls[calls.length - 1]?.[0]).toContain('c("dplyr")');
    vi.mocked(invoke).mockReset();
  });
  it("大量结果按页展示且按钮 Enter 不触发表格复制", async () => {
    const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
    const results: SearchResult[] = Array.from({ length: 205 }, (_, index) => ({ package: `pkg${String(index).padStart(3, "0")}`, requestedVersion: "", latestVersion: "", repository: "", realName: `pkg${index}`, source: "none", found: false, message: "" }));
    render(<ReportView results={results} logs={[]} dependencyGraph={null} packageCount={205} uniqueFoundCount={0} smartSuggestions={[]} searching={false} searchDuration={1} onClearLogs={vi.fn()} onStatusChange={vi.fn()} onApplySmartSuggestion={vi.fn()} onRetryMissing={vi.fn()} />);
    expect(screen.getAllByRole("row")).toHaveLength(101);
    const next = screen.getByRole("button", { name: "下一页" });
    vi.mocked(writeText).mockClear();
    fireEvent.keyDown(next, { key: "Enter" });
    expect(writeText).not.toHaveBeenCalled();
    fireEvent.click(next);
    expect(screen.getByText("pkg100")).toBeInTheDocument();
    expect(screen.queryByText("pkg000")).not.toBeInTheDocument();
  });
  const mockResults: SearchResult[] = [
    {
      package: "dplyr",
      requestedVersion: "",
      latestVersion: "1.1.2",
      repository: "https://cran.r-project.org",
      realName: "dplyr",
      source: "cran",
      found: true,
      message: "验证成功",
      status: "found",
    },
    {
      package: "nonexist",
      requestedVersion: "",
      latestVersion: "",
      repository: "",
      realName: "nonexist",
      source: "none",
      found: false,
      message: "所有来源均未找到",
      status: "notFound",
    },
    {
      package: "slowpkg",
      requestedVersion: "",
      latestVersion: "",
      repository: "",
      realName: "slowpkg",
      source: "unknown",
      found: false,
      message: "请求超时",
      status: "timeout",
    },
    {
      package: "limitedpkg",
      requestedVersion: "",
      latestVersion: "",
      repository: "",
      realName: "limitedpkg",
      source: "github",
      found: false,
      message: "频率限制",
      status: "rateLimited",
    },
    {
      package: "brokenpkg",
      requestedVersion: "",
      latestVersion: "",
      repository: "",
      realName: "brokenpkg",
      source: "unknown",
      found: false,
      message: "检索异常",
      status: "error",
    },
  ];

  const cachedResults: SearchResult[] = [
    {
      package: "ggplot2",
      requestedVersion: "",
      latestVersion: "3.5.0",
      repository: "",
      realName: "ggplot2",
      source: "cran",
      found: true,
      message: "缓存命中",
      status: "found",
      stage: "cacheHit",
    },
    {
      package: "limitedpkg",
      requestedVersion: "",
      latestVersion: "",
      repository: "",
      realName: "limitedpkg",
      source: "github",
      found: false,
      message: "频率限制",
      status: "rateLimited",
    },
  ];

  it("renders report overview correctly", () => {
    render(
      <ReportView
        results={mockResults}
        logs={["log1", "log2"]}
        dependencyGraph={null}
        packageCount={5}
        uniqueFoundCount={1}
        smartSuggestions={[]}
        searching={false}
        searchDuration={1200}
        stageTimings={[{ stage: "多源检索", durationMs: 800 }, { stage: "依赖解析", durationMs: 400 }]}
        onClearLogs={() => {}}
        onStatusChange={() => {}}
        onApplySmartSuggestion={() => {}}
        onRetryMissing={() => {}}
      />
    );

    expect(screen.getByText("dplyr")).toBeInTheDocument();
    expect(screen.getByText("nonexist")).toBeInTheDocument();
    expect(screen.getByText("1.1.2")).toBeInTheDocument();
  });

  it("filters results by status when metrics are clicked", () => {
    render(
      <ReportView
        results={mockResults}
        logs={[]}
        dependencyGraph={null}
        packageCount={2}
        uniqueFoundCount={1}
        smartSuggestions={[]}
        searching={false}
        searchDuration={1200}
        onClearLogs={() => {}}
        onStatusChange={() => {}}
        onApplySmartSuggestion={() => {}}
        onRetryMissing={() => {}}
      />
    );

    // Click "未找到" to filter, it might be a button metric
    const metricBtns = screen.getAllByRole("button");
    const missingBtn = metricBtns.find(btn => btn.textContent?.includes("未找到"));
    if (missingBtn) fireEvent.click(missingBtn);
    expect(screen.queryByText("dplyr")).not.toBeInTheDocument();
    expect(screen.getAllByText("nonexist").length).toBeGreaterThan(0);
  });

  it("triggers onRetryMissing when retry button is clicked", () => {
    const handleRetry = vi.fn();
    render(
      <ReportView
        results={mockResults}
        logs={[]}
        dependencyGraph={null}
        packageCount={2}
        uniqueFoundCount={1}
        smartSuggestions={[]}
        searching={false}
        searchDuration={1200}
        onClearLogs={() => {}}
        onStatusChange={() => {}}
        onApplySmartSuggestion={() => {}}
        onRetryMissing={handleRetry}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: "重试全部失败" }));
    expect(handleRetry).toHaveBeenCalledWith(["nonexist", "slowpkg", "limitedpkg", "brokenpkg"]);
  });

  it("retries a specific failure category from failure breakdown", () => {
    const handleRetry = vi.fn();
    const handleStatus = vi.fn();
    render(
      <ReportView
        results={mockResults}
        logs={[]}
        dependencyGraph={null}
        packageCount={5}
        uniqueFoundCount={1}
        smartSuggestions={[]}
        searching={false}
        searchDuration={1200}
        onClearLogs={() => {}}
        onStatusChange={handleStatus}
        onApplySmartSuggestion={() => {}}
        onRetryMissing={handleRetry}
      />
    );

    expect(screen.getByLabelText("失败分类摘要")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /超时 1/ }));
    expect(handleRetry).toHaveBeenCalledWith(["slowpkg"]);
    expect(handleStatus).toHaveBeenCalledWith("已回填 1 个超时包，可重新检索");
  });

  it("deduplicates failure retries case-insensitively", () => {
    const handleRetry = vi.fn();
    render(
      <ReportView
        results={[
          ...mockResults,
          { ...mockResults[1], package: "NONEXIST" },
        ]}
        logs={[]}
        dependencyGraph={null}
        packageCount={5}
        uniqueFoundCount={1}
        smartSuggestions={[]}
        searching={false}
        searchDuration={1200}
        onClearLogs={() => {}}
        onStatusChange={() => {}}
        onApplySmartSuggestion={() => {}}
        onRetryMissing={handleRetry}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: "重试全部失败" }));
    expect(handleRetry).toHaveBeenCalledWith(["nonexist", "slowpkg", "limitedpkg", "brokenpkg"]);
  });

  it("renders task summary with cache hits and next action", () => {
    render(
      <ReportView
        results={cachedResults}
        logs={[]}
        dependencyGraph={null}
        packageCount={2}
        uniqueFoundCount={1}
        smartSuggestions={[]}
        searching={false}
        searchDuration={1500}
        onClearLogs={() => {}}
        onStatusChange={() => {}}
        onApplySmartSuggestion={() => {}}
        onRetryMissing={() => {}}
      />
    );

    const summary = screen.getByLabelText("任务摘要");
    expect(summary).toHaveTextContent("2/2 个包已返回结果");
    expect(summary).toHaveTextContent("验证率 50%");
    expect(summary).toHaveTextContent("缓存命中 1");
    expect(summary).toHaveTextContent("限流 1");
    expect(summary).toHaveTextContent("1.5s");
    expect(summary).toHaveTextContent("建议配置 GitHub Token");
  });

  it("uses the requested version in the copied CRAN command and shows version difference", async () => {
    const writeText = await import("@tauri-apps/plugin-clipboard-manager").then((module) => module.writeText);
    const requested = { ...mockResults[0], requestedVersion: "1.0.0", latestVersion: "1.1.2" };
    render(
      <ReportView
        results={[requested]}
        logs={[]}
        dependencyGraph={null}
        packageCount={1}
        uniqueFoundCount={1}
        smartSuggestions={[]}
        searching={false}
        searchDuration={1200}
        onClearLogs={() => {}}
        onStatusChange={() => {}}
        onApplySmartSuggestion={() => {}}
        onRetryMissing={() => {}}
      />
    );

    expect(screen.getByText(/1\.1\.2（请求 1\.0\.0）/)).toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByTitle(/复制安装指令/)[0].getAttribute("title")).not.toBe("复制安装指令: "));
    await act(async () => {
      fireEvent.click(screen.getAllByTitle(/复制安装指令/)[0]);
    });
    expect(vi.mocked(writeText)).toHaveBeenCalledWith(expect.stringContaining('install_version("dplyr", version = "1.0.0"'));
  });

  it("uses latestVersion in conda command when no requested version", async () => {
    const writeText = await import("@tauri-apps/plugin-clipboard-manager").then((module) => module.writeText);
    const condaResult: SearchResult = {
      package: "numpy",
      requestedVersion: "",
      latestVersion: "1.26.4",
      repository: "conda-forge",
      realName: "numpy",
      source: "conda",
      found: true,
      message: "检索成功",
      status: "found",
    };
    render(
      <ReportView
        results={[condaResult]}
        logs={[]}
        dependencyGraph={null}
        packageCount={1}
        uniqueFoundCount={1}
        smartSuggestions={[]}
        searching={false}
        searchDuration={500}
        onClearLogs={() => {}}
        onStatusChange={() => {}}
        onApplySmartSuggestion={() => {}}
        onRetryMissing={() => {}}
      />
    );

    await act(async () => {
      fireEvent.click(screen.getAllByTitle(/复制安装指令/)[0]);
    });
    expect(vi.mocked(writeText)).toHaveBeenCalledWith(
      expect.stringContaining("conda 'install' '-c' 'conda-forge' 'numpy==1.26.4'"),
    );
  });

  it("uses latestVersion in pip command when no requested version", async () => {
    const writeText = await import("@tauri-apps/plugin-clipboard-manager").then((module) => module.writeText);
    const pipResult: SearchResult = {
      package: "requests",
      requestedVersion: "",
      latestVersion: "2.31.0",
      repository: "https://pypi.org",
      realName: "requests",
      source: "pip",
      found: true,
      message: "检索成功",
      status: "found",
    };
    render(
      <ReportView
        results={[pipResult]}
        logs={[]}
        dependencyGraph={null}
        packageCount={1}
        uniqueFoundCount={1}
        smartSuggestions={[]}
        searching={false}
        searchDuration={500}
        onClearLogs={() => {}}
        onStatusChange={() => {}}
        onApplySmartSuggestion={() => {}}
        onRetryMissing={() => {}}
      />
    );

    await act(async () => {
      fireEvent.click(screen.getAllByTitle(/复制安装指令/)[0]);
    });
    expect(vi.mocked(writeText)).toHaveBeenCalledWith(
      expect.stringContaining("pip 'install' 'requests==2.31.0'"),
    );
  });
});
