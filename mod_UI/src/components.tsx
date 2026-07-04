import type { ReactNode } from "react";

export function NavButton({
  active,
  label,
  code,
  badge,
  onClick,
}: {
  active: boolean;
  label: string;
  code: string;
  badge?: number;
  onClick: () => void;
}) {
  return (
    <button
      className={active ? "active" : ""}
      aria-current={active ? "page" : undefined}
      onClick={onClick}
    >
      <span className="nav-code">{code}</span>
      <strong>{label}</strong>
      {badge !== undefined && badge > 0 && <small>{badge}</small>}
    </button>
  );
}

export function PanelHeader({
  step,
  title,
  meta,
}: {
  step: string;
  title: string;
  meta: string;
}) {
  return (
    <header className="panel-header">
      <span>{step}</span>
      <h2>{title}</h2>
      <small>{meta}</small>
    </header>
  );
}

export function Toggle({
  checked,
  label,
  description,
  onChange,
}: {
  checked: boolean;
  label: string;
  description: string;
  onChange: (value: boolean) => void;
}) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.currentTarget.checked)} />
      <span className="toggle-control"><i /></span>
      <span><strong>{label}</strong><small>{description}</small></span>
    </label>
  );
}

export function Metric({
  label,
  value,
  tone = "",
  active = false,
  onClick,
}: {
  label: string;
  value: number;
  tone?: string;
  active?: boolean;
  onClick?: () => void;
}) {
  const clickable = onClick !== undefined;
  const Tag = clickable ? "button" : "div";
  return (
    <Tag
      type={clickable ? "button" : undefined}
      className={`metric ${tone} ${active ? "active" : ""} ${clickable ? "clickable" : ""}`}
      onClick={onClick}
    >
      <span>{label}</span>
      <strong>{value}</strong>
    </Tag>
  );
}

export function EmptyState({ text, hint }: { text: string; hint?: string }) {
  return (
    <div className="empty-state">
      <span>—</span>
      {text}
      {hint && <small className="empty-hint">{hint}</small>}
    </div>
  );
}

export function PanelHeaderWithChildren({
  step,
  title,
  meta,
  children,
}: {
  step: string;
  title: string;
  meta: string;
  children?: ReactNode;
}) {
  return (
    <header className="panel-header">
      <span>{step}</span>
      <h2>{title}</h2>
      <small>{meta}</small>
      {children}
    </header>
  );
}
