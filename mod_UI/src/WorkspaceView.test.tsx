import { describe, it, expect, vi } from "vitest";
import "@testing-library/jest-dom";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { WorkspaceView } from "./WorkspaceView";
import { defaultSettings, type Method } from "./types";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(),
}));

describe("WorkspaceView", () => {
  const defaultProps = {
    input: "dplyr\ntidyr",
    inputTooLarge: false,
    inputProfile: { total: 2, archiveUrls: 0, repositories: 0 },
    method: "auto" as Method,
    conditional: false,
    installDependencies: false,
    showRemoteVersion: false,
    verifyInstall: false,
    parallelInstall: false,
    settings: {
      ...defaultSettings,
      fullSearch: false,
      useCache: false,
      proxy: "",
      githubToken: "",
    },
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
    pinnedMethods: ["auto" as Method],
    onPinnedMethodsChange: vi.fn(),
    onApplySmartSuggestion: vi.fn(),
    onConditionalChange: vi.fn(),
    onInstallDependenciesChange: vi.fn(),
    onShowRemoteVersionChange: vi.fn(),
    onVerifyInstallChange: vi.fn(),
    onParallelInstallChange: vi.fn(),
    onFullSearchChange: vi.fn(),
    onUseCacheChange: vi.fn(),
    onTempFilter: vi.fn(),
    onCopyScript: vi.fn(),
    onCleanComments: vi.fn(),
    onDownloadScript: vi.fn(),
    onDownloadPowerShellScript: vi.fn(),
    onDownloadBashScript: vi.fn(),
    onDownloadSystemRequirements: vi.fn(),
    copyWithLineNumbers: false,
    onCopyWithLineNumbersChange: vi.fn(),
    isMethodDisabled: () => false,
  };

  it("renders textarea with correct input", () => {
    render(<WorkspaceView {...defaultProps} />);
    const textarea = screen.getByRole("textbox", { name: "R 包输入列表" });
    expect(textarea).toHaveValue("dplyr\ntidyr");
  });

  it("preserves multiline input when Enter is pressed", () => {
    const handleInputChange = vi.fn();
    render(<WorkspaceView {...defaultProps} input="dplyr" onInputChange={handleInputChange} />);
    const textarea = screen.getByRole("textbox", { name: "R 包输入列表" }) as HTMLTextAreaElement;
    textarea.setSelectionRange(textarea.value.length, textarea.value.length);

    fireEvent.keyDown(textarea, { key: "Enter" });

    expect(handleInputChange).toHaveBeenCalledWith("dplyr\n", "manual");
  });

  it("starts search with Ctrl+Enter without changing multiline input", () => {
    const handleStartSearch = vi.fn();
    const handleInputChange = vi.fn();
    render(<WorkspaceView {...defaultProps} onStartSearch={handleStartSearch} onInputChange={handleInputChange} />);
    const textarea = screen.getByRole("textbox", { name: "R 包输入列表" });

    fireEvent.keyDown(textarea, { key: "Enter", ctrlKey: true });

    expect(handleStartSearch).toHaveBeenCalledOnce();
    expect(handleInputChange).not.toHaveBeenCalled();
  });

  it("does not start search for Ctrl+Enter while composing", () => {
    const handleStartSearch = vi.fn();
    render(<WorkspaceView {...defaultProps} onStartSearch={handleStartSearch} />);
    const textarea = screen.getByRole("textbox", { name: "R 包输入列表" });

    fireEvent.keyDown(textarea, { key: "Enter", ctrlKey: true, isComposing: true });

    expect(handleStartSearch).not.toHaveBeenCalled();
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

  it("renders search plan preview for current input", () => {
    render(<WorkspaceView {...defaultProps} />);
    const plan = screen.getByLabelText("搜索计划预览");

    expect(plan).toBeInTheDocument();
    expect(within(plan).getByText(/2 个输入/)).toBeInTheDocument();
    expect(within(plan).getByText(/预计/)).toBeInTheDocument();
    expect(within(plan).getByText("快速检索")).toBeInTheDocument();
  });

  it("marks large full searches as high intensity", () => {
    render(
      <WorkspaceView
        {...defaultProps}
        input={Array.from({ length: 90 }, (_, i) => `pkg${i}`).join("\n")}
        inputProfile={{ total: 90, archiveUrls: 0, repositories: 0 }}
        settings={{ ...defaultProps.settings, fullSearch: true, useCache: true }}
      />,
    );

    const plan = screen.getByLabelText("搜索计划预览");
    expect(within(plan).getByText("全量检索")).toBeInTheDocument();
    expect(within(plan).getByText("高")).toBeInTheDocument();
    expect(within(plan).getByText(/GitHub Token/)).toBeInTheDocument();
  });

  it("triggers copy script callback", () => {
    const handleCopy = vi.fn();
    render(<WorkspaceView {...defaultProps} onCopyScript={handleCopy} />);
    fireEvent.click(screen.getByText(/复制脚本/));
    expect(handleCopy).toHaveBeenCalled();
  });

  it("triggers wrapper script download callbacks", () => {
    const onPowerShell = vi.fn();
    const onBash = vi.fn();
    render(<WorkspaceView {...defaultProps} onDownloadPowerShellScript={onPowerShell} onDownloadBashScript={onBash} />);

    fireEvent.click(screen.getByText("下载 .ps1"));
    fireEvent.click(screen.getByText("下载 .sh"));

    expect(onPowerShell).toHaveBeenCalledOnce();
    expect(onBash).toHaveBeenCalledOnce();
  });
});
