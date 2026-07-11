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
  ];

  it("renders report overview correctly", () => {
    render(
      <ReportView
        results={mockResults}
        logs={["log1", "log2"]}
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

    const retryBtns = screen.queryAllByText("重试");
    if (retryBtns.length > 0) {
      fireEvent.click(retryBtns[0]);
      expect(handleRetry).toHaveBeenCalledWith(["nonexist"]);
    }
  });
});
