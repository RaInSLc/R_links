import { describe, it, expect, vi } from "vitest";
import "@testing-library/jest-dom";
import { render, screen, fireEvent } from "@testing-library/react";
import { HistoryView } from "./HistoryView";

// Mock @tauri-apps/plugin-clipboard-manager
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(),
}));

describe("HistoryView", () => {
  const mockHistory = [
    {
      id: "1",
      packageName: "dplyr",
      toolName: "RLinks",
      command: "install.packages('dplyr')",
      createdAt: "2026-07-11T10:00:00Z",
    },
    {
      id: "2",
      packageName: "tidyr",
      toolName: "RLinks",
      command: "install.packages('tidyr')",
      createdAt: "2026-07-11T10:05:00Z",
    },
  ];

  it("renders history records correctly", () => {
    render(
      <HistoryView
        history={mockHistory}
        historySearch=""
        onHistorySearchChange={() => {}}
        onApplyRecord={() => {}}
        onCopyRecord={() => {}}
        onDeleteRecord={() => {}}
        onClearAll={() => {}}
      />
    );

    expect(screen.getByText("dplyr")).toBeInTheDocument();
    expect(screen.getByText("tidyr")).toBeInTheDocument();
  });

  it("filters records based on search input", () => {
    render(
      <HistoryView
        history={mockHistory}
        historySearch="dplyr"
        onHistorySearchChange={() => {}}
        onApplyRecord={() => {}}
        onCopyRecord={() => {}}
        onDeleteRecord={() => {}}
        onClearAll={() => {}}
      />
    );

    expect(screen.getByText("dplyr")).toBeInTheDocument();
    expect(screen.queryByText("tidyr")).not.toBeInTheDocument();
  });

  it("calls onDeleteRecord when delete button is clicked", () => {
    const handleDelete = vi.fn();
    render(
      <HistoryView
        history={mockHistory}
        historySearch=""
        onHistorySearchChange={() => {}}
        onApplyRecord={() => {}}
        onCopyRecord={() => {}}
        onDeleteRecord={handleDelete}
        onClearAll={() => {}}
      />
    );

    const deleteBtns = screen.getAllByText("删除");
    fireEvent.click(deleteBtns[0]);
    expect(handleDelete).toHaveBeenCalledWith("1");
  });

  it("calls onClearAll when clear all is clicked and confirmed", () => {
    vi.spyOn(window, "confirm").mockImplementation(() => true);
    const handleClearAll = vi.fn();
    render(
      <HistoryView
        history={mockHistory}
        historySearch=""
        onHistorySearchChange={() => {}}
        onApplyRecord={() => {}}
        onCopyRecord={() => {}}
        onDeleteRecord={() => {}}
        onClearAll={handleClearAll}
      />
    );

    fireEvent.click(screen.getByText("清空全部"));
    expect(handleClearAll).toHaveBeenCalled();
  });
});
