import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import "@testing-library/jest-dom";
import { SidebarVersion } from "./SidebarVersion";
import type { UpdateFailureStage, UpdateState } from "./types";
import { BUILD_APP_VERSION } from "./utils-update";

const createProps = (overrides: {
  appVersion?: string;
  updateState?: UpdateState;
  updateStage?: UpdateFailureStage | null;
} = {}) => ({
  appVersion: "1.2.3",
  updateState: "idle" as UpdateState,
  updateStage: null as UpdateFailureStage | null,
  updaterConfig: null,
  onOpenUpdateSettings: vi.fn(),
  ...overrides,
});

describe("SidebarVersion", () => {
  it("常驻显示当前版本号", () => {
    render(<SidebarVersion {...createProps()} />);
    expect(screen.getByText("v1.2.3")).toBeInTheDocument();
    expect(screen.getByText("版本")).toBeInTheDocument();
  });

  it("运行时版本为空时回退到构建期注入的版本，而不是空白", () => {
    render(<SidebarVersion {...createProps({ appVersion: "" })} />);
    expect(screen.getByText(`v${BUILD_APP_VERSION}`)).toBeInTheDocument();
  });

  it("点击徽标打开更新设置", () => {
    const props = createProps();
    render(<SidebarVersion {...props} />);
    fireEvent.click(screen.getByRole("button"));
    expect(props.onOpenUpdateSettings).toHaveBeenCalledTimes(1);
  });

  it("失败时同时显示状态与失败阶段，便于直接定位", () => {
    render(
      <SidebarVersion
        {...createProps({ updateState: "error", updateStage: "manifest-missing" })}
      />,
    );
    expect(screen.getByText("检查失败 · 更新清单缺失")).toBeInTheDocument();
  });

  it("状态点带有可区分更新状态的数据标记", () => {
    const { container } = render(<SidebarVersion {...createProps({ updateState: "upToDate" })} />);
    const dot = container.querySelector(".version-dot");
    expect(dot).not.toBeNull();
    expect(dot?.className).toContain("upToDate");
  });

  it("提示文案里带上真正生效的更新端点", () => {
    render(
      <SidebarVersion
        {...createProps()}
        updaterConfig={{
          endpoints: ["https://example.com/latest.json"],
          pubkeyKeyId: "ABCDEF0123456789",
          currentVersion: "1.2.3",
        }}
      />,
    );
    expect(screen.getByRole("button").getAttribute("title")).toContain("https://example.com/latest.json");
  });
});
