import { type ReactNode, useState } from "react";
import { AlertCircle, Check, CheckCircle2, Copy, LoaderCircle, X } from "lucide-react";
import { t } from "./locale";

export function Badge({ children, tone = "neutral" }: { children: ReactNode; tone?: string }) {
  return <span className={`badge ${tone}`}><i />{children}</span>;
}

export function Card({ children, title, action, className = "" }: { children: ReactNode; title?: string; action?: ReactNode; className?: string }) {
  return <section className={`card ${className}`}>{title && <div className="card-head"><h2>{title}</h2>{action}</div>}{children}</section>;
}

export function Empty({ text, detail }: { text: string; detail?: string }) {
  return <div className="empty"><AlertCircle size={18} /><div><p>{text}</p>{detail && <small>{detail}</small>}</div></div>;
}

export function Loading({ text = t("state.loading") }: { text?: string }) {
  return <div className="loading"><LoaderCircle size={18} className="spin" /><span>{text}</span></div>;
}

export function ErrorState({ error, retry }: { error: string; retry: () => void }) {
  return <main className="content state-page"><Card><div className="state"><X size={25} /><h1>{t("state.error")}</h1><p>{error}</p><button className="button primary" onClick={retry}>{t("state.retry")}</button></div></Card></main>;
}

export function CopyButton({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  };
  return <button className="icon copy-button" title={copied ? t("common.copied") : t("common.copy")} aria-label={copied ? t("common.copied") : t("common.copy")} onClick={copy}>{copied ? <Check size={14} /> : <Copy size={14} />}</button>;
}

export function Status({ value }: { value?: string }) {
  const tone = value === "running" ? "warning" : value === "success" || value === "done" ? "online" : value === "failed" || value === "blocked" || value === "error" ? "danger" : "neutral";
  return <Badge tone={tone}>{value ?? t("common.notRecorded")}</Badge>;
}

export function Confirm({ title, children, onConfirm, onClose, busy = false }: { title: string; children: ReactNode; onConfirm: () => void; onClose: () => void; busy?: boolean }) {
  return <div className="modal-backdrop" onClick={onClose}><div className="modal confirm" role="dialog" aria-modal="true" aria-labelledby="confirm-title" onClick={e => e.stopPropagation()}><button className="close" onClick={onClose} aria-label={t("window.close")}><X size={16} /></button><h2 id="confirm-title">{title}</h2><p>{children}</p><div className="modal-actions"><button className="button" onClick={onClose} disabled={busy}>{t("tasks.keep")}</button><button className="button danger" onClick={onConfirm} disabled={busy}>{busy ? t("tasks.cancelling") : t("tasks.confirmCancel")}</button></div></div></div>;
}

export function fmt(value?: string) { return value ? new Date(value).toLocaleString("zh-CN") : t("common.notRecorded"); }
export function CheckMark() { return <Check size={15} />; }
export function SaveNotice({ text = t("common.saved") }: { text?: string }) { return <span className="save-notice"><CheckCircle2 size={14} />{text}</span>; }
