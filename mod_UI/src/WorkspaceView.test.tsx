import { describe, it, expect, vi } from "vitest";
import "@testing-library/jest-dom";
import { render, screen, fireEvent } from "@testing-library/react";
import { WorkspaceView } from "./WorkspaceView";
import type { Method, Settings } from "./types";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(),
}));

describe("WorkspaceView", () => {
  const defaultProps = {
    input: "dplyr\ntidyr",
    inputTooLarge: false,
    inputProfile: { total: 2, archiveUrls: 0, repositories: 0 },
    method: "normal" as Method,
    conditional: false,
    installDependencies: false,
    showRemoteVersion: false,
    verifyInstall: false,
    settings: {
      fullSearch: false,
      useCache: false,
      proxy: "",
      githubToken: "",
      biocVersionFallback: "",
    } as Settings,
    smartSuggestions: [],
    script: "install.packages(c('dplyr', 'tidyr'))",
    scriptTooLarge: false,
    scriptCommandCount: 1,
    duplicateCount: 0,
    searching: false,
    openingSearchTabs: false,
    onInputChange: vi.fn(),
    onPaste: vi.fn(),
    onClear: vi.fn(),
    onOpenSearchTabs: vi.fn(),
    onStartSearch: vi.fn(),
    onStopSearch: vi.fn(),
    onMethodChange: vi.fn(),
    pinnedMethods: ["normal" as Method],
    onPinnedMethodsChange: vi.fn(),
    onApplySmartSuggestion: vi.fn(),
    onConditionalChange: vi.fn(),
    onInstallDependenciesChange: vi.fn(),
    onShowRemoteVersionChange: vi.fn(),
    onVerifyInstallChange: vi.fn(),
    onFullSearchChange: vi.fn(),
    onUseCacheChange: vi.fn(),
    onTempFilter: vi.fn(),
    onCopyScript: vi.fn(),
    onCleanComments: vi.fn(),
    onDownloadScript: vi.fn(),
    copyWithLineNumbers: false,
    onCopyWithLineNumbersChange: vi.fn(),
    isMethodDisabled: () => false,
  };

  it("renders textarea with correct input", () => {
    render(<WorkspaceView {...defaultProps} />);
    const textarea = screen.getByRole("textbox", { name: "R 包输入列表" });
    expect(textarea).toHaveValue("dplyr\ntidyr");
  });

  it("calls onClear when clear button is clicked", () => {
    const handleClear = vi.fn();
    render(<WorkspaceView {...defaultProps} onClear={handleClear} />);
    fireEvent.click(screen.getByText("清空"));
    expect(handleClear).toHaveBeenCalled();
  });

  it("calls onStartSearch when search button is clicked", () => {
    const handleStartSearch = vi.fn();
    render(<WorkspaceView {...defaultProps} onStartSearch={handleStartSearch} />);
    fireEvent.click(screen.getByText(/开始检索/));
    expect(handleStartSearch).toHaveBeenCalled();
  });

  it("renders script preview correctly", () => {
    const { container } = render(<WorkspaceView {...defaultProps} />);
    expect(container.textContent).toContain("install.packages");
    expect(container.textContent).toContain("'dplyr'");
  });

  it("triggers copy script callback", () => {
    const handleCopy = vi.fn();
    render(<WorkspaceView {...defaultProps} onCopyScript={handleCopy} />);
    fireEvent.click(screen.getByText(/复制脚本/));
    expect(handleCopy).toHaveBeenCalled();
  });
});
