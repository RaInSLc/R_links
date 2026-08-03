import { Component, type ErrorInfo, type ReactNode } from "react";

export class AppErrorBoundary extends Component<{ children: ReactNode }, { message: string }> {
  state = { message: "" };

  static getDerivedStateFromError(error: unknown) {
    return { message: error instanceof Error ? error.message : String(error) };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error("App render failed", error, info.componentStack);
  }

  render() {
    if (this.state.message) {
      return (
        <div className="app-shell" style={{ display: "grid", placeItems: "center", minHeight: "100vh", padding: "24px" }}>
          <section className="panel" style={{ maxWidth: "640px", width: "100%" }}>
            <header className="panel-header">
              <span>错误</span>
              <h2>界面渲染失败</h2>
              <small>已阻止白屏</small>
            </header>
            <div style={{ padding: "16px", display: "grid", gap: "12px" }}>
              <p>界面遇到运行时错误，建议先刷新应用；如果刚修改了缓存或设置，请导出诊断后反馈。</p>
              <code style={{ whiteSpace: "pre-wrap" }}>{this.state.message}</code>
              <button className="button primary" type="button" onClick={() => window.location.reload()}>刷新应用</button>
            </div>
          </section>
        </div>
      );
    }
    return this.props.children;
  }
}
