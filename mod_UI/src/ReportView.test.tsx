import { describe, it, expect, vi } from "vitest";
import "@testing-library/jest-dom";
import { render, screen, fireEvent } from "@testing-library/react";
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
});
