import { useEffect, useState } from "react";
import { PanelHeader, Toggle } from "./components";
import type { Method, Settings } from "./types";
import { methods, defaultPinnedMethods } from "./types";

interface WorkspaceStrategyPanelProps {
  ecosystem: string;
  method: Method;
  settings: Settings;
  conditional: boolean;
  installDependencies: boolean;
  showRemoteVersion: boolean;
  verifyInstall: boolean;
  parallelInstall: boolean;
  searching: boolean;
  pinnedMethods: Method[];
  onMethodChange: (method: Method) => void;
  onPinnedMethodsChange: (methods: Method[]) => void;
  onConditionalChange: (value: boolean) => void;
  onInstallDependenciesChange: (value: boolean) => void;
  onShowRemoteVersionChange: (value: boolean) => void;
  onVerifyInstallChange: (value: boolean) => void;
  onParallelInstallChange: (value: boolean) => void;
  onFullSearchChange: (value: boolean) => void;
  onUseCacheChange: (value: boolean) => void;
  isMethodDisabled: (candidate: Method) => boolean;
}

export function WorkspaceStrategyPanel({
  ecosystem, method, settings, conditional, installDependencies, showRemoteVersion,
  verifyInstall, parallelInstall, searching, pinnedMethods, onMethodChange,
  onPinnedMethodsChange, onConditionalChange, onInstallDependenciesChange,
  onShowRemoteVersionChange, onVerifyInstallChange, onParallelInstallChange,
  onFullSearchChange, onUseCacheChange, isMethodDisabled,
}: WorkspaceStrategyPanelProps) {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open) return;
    function onKeydown(event: KeyboardEvent) {
      if (event.key === "Escape") { event.preventDefault(); setOpen(false); }
    }
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  }, [open]);

  return (
    <section className={`panel method-panel compact-method-panel ${ecosystem !== "r" ? "multi-method-panel" : ""}`}>
      <PanelHeader step="02" title={ecosystem === "r-binary" ? "二进制命令" : "安装策略"} meta={ecosystem === "r-binary" ? "不执行网络检索" : settings.fullSearch ? "全量检索" : "快速检索"} />
      {ecosystem !== "r" && <div className="multi-ecosystem-note"><strong>{ecosystem === "r-binary" ? "R 二进制命令生成" : ecosystem === "pip" ? "Pip 批量检索" : "Conda 批量检索"}</strong><span>{ecosystem === "r-binary" ? "根据输入直接生成 RSPM 安装代码，不混合默认 R 多源搜索。" : "源地址和版本会写入检索结果，复制命令即可安装。"}</span></div>}
      <div className="method-grid pinned-method-grid" aria-label="常用安装策略">
        {pinnedMethods.map((id) => {
          const item = methods.find((m) => m.id === id);
          if (!item) return null;
          return <button key={item.id} className={`method-card ${method === item.id ? "selected" : ""}`} disabled={isMethodDisabled(item.id)} aria-pressed={method === item.id} onClick={() => onMethodChange(item.id)}><span>{item.title}</span><small>{item.description}</small></button>;
        })}
      </div>
      <div className="strategy-footer">
        <div className="strategy-chips" aria-label="当前策略选项">
          {conditional && <span>条件安装</span>}
          {installDependencies && <span>安装依赖</span>}
          {showRemoteVersion && <span>同步版本</span>}
          {settings.fullSearch && <span>全量检索</span>}
          {settings.useCache && <span>使用缓存</span>}
          {verifyInstall && <span>安装后验证</span>}
        </div>
        <button type="button" className="button ghost compact-btn" onClick={() => setOpen(true)}>
          配置策略
        </button>
      </div>
      <StrategyDrawer
        open={open}
        method={method}
        settings={settings}
        conditional={conditional}
        installDependencies={installDependencies}
        showRemoteVersion={showRemoteVersion}
        verifyInstall={verifyInstall}
        parallelInstall={parallelInstall}
        pinnedMethods={pinnedMethods}
        searching={searching}
        onMethodChange={onMethodChange}
        onPinnedMethodsChange={onPinnedMethodsChange}
        onConditionalChange={onConditionalChange}
        onInstallDependenciesChange={onInstallDependenciesChange}
        onShowRemoteVersionChange={onShowRemoteVersionChange}
        onVerifyInstallChange={onVerifyInstallChange}
        onParallelInstallChange={onParallelInstallChange}
        onFullSearchChange={onFullSearchChange}
        onUseCacheChange={onUseCacheChange}
        isMethodDisabled={isMethodDisabled}
        onClose={() => setOpen(false)}
      />
    </section>
  );
}

interface StrategyDrawerProps extends Omit<WorkspaceStrategyPanelProps, "ecosystem"> { open: boolean; onClose?: () => void }

function StrategyDrawer({ open, onClose = () => {}, ...props }: StrategyDrawerProps) {
  if (!open) return null;
  return <div className="strategy-overlay" role="presentation" onClick={onClose}><section className="panel strategy-drawer" role="dialog" aria-modal="true" aria-label="安装策略配置" onClick={(event) => event.stopPropagation()}><PanelHeader step="02" title="安装策略" meta={props.settings.fullSearch ? "全量检索" : "快速检索"} /><div className="method-grid">{methods.map((item) => <button key={item.id} className={`method-card ${props.method === item.id ? "selected" : ""}`} disabled={props.isMethodDisabled(item.id)} aria-pressed={props.method === item.id} onClick={() => props.onMethodChange(item.id)}><span>{item.title}</span><small>{item.description}</small></button>)}</div><div className="pin-section"><p className="pin-section-title">面板常用策略</p><div className="pin-chips">{methods.map((item) => { const pinned = props.pinnedMethods.includes(item.id); return <button key={item.id} type="button" className={`pin-chip ${pinned ? "active" : ""}`} onClick={() => { if (pinned) { if (props.pinnedMethods.length > 1) props.onPinnedMethodsChange(props.pinnedMethods.filter((m) => m !== item.id)); } else props.onPinnedMethodsChange([...props.pinnedMethods, item.id]); }} aria-pressed={pinned} title={pinned ? "从面板移除" : "添加到面板"}>{item.title}</button>; })}</div>{props.pinnedMethods.length < defaultPinnedMethods.length && <button type="button" className="text-button pin-reset" onClick={() => props.onPinnedMethodsChange([...defaultPinnedMethods])}>恢复默认常用</button>}</div><div className="toggle-row"><Toggle checked={props.conditional} label="条件安装" description="已安装时自动跳过" onChange={props.onConditionalChange} /><Toggle checked={props.installDependencies} label="安装依赖" description="dependencies = TRUE" onChange={props.onInstallDependenciesChange} /><Toggle checked={props.showRemoteVersion} label="同步远程版本" description="显示版本并生成精确版本安装" onChange={props.onShowRemoteVersionChange} /><Toggle checked={props.settings.fullSearch} label="全量检索" description="命中后仍继续查询 GitHub" onChange={props.onFullSearchChange} /><Toggle checked={props.settings.useCache} label="使用缓存" description="使用包结果缓存" onChange={props.onUseCacheChange} /><Toggle checked={props.verifyInstall} label="安装后验证" description="脚本末尾追加安装结果验证代码" onChange={props.onVerifyInstallChange} /><Toggle checked={props.parallelInstall} label="多核编译" description="启用 parallel::detectCores() 加速源码包安装" onChange={props.onParallelInstallChange} /></div><div className="strategy-drawer-actions"><button type="button" className="button primary" onClick={onClose}>完成</button></div></section></div>;
}
