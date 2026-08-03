import { describe, it, expect, vi } from "vitest";
import "@testing-library/jest-dom";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { ReportView } from "./ReportView";
import type { SearchResult } from "./utils";

// Mock @tauri-apps/api/core and @tauri-apps/plugin-clipboard-manager
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(),
}));

describe("ReportView", () => {
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
      expect.stringContaining("conda install conda-forge::numpy=1.26.4"),
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
      expect.stringContaining("pip install requests==2.31.0"),
    );
  });
});
