// web/src/components/ui.tsx
import type {
  ButtonHTMLAttributes,
  CSSProperties,
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
} from 'react';
import { theme } from '../theme';

export type Tone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger' | 'info';

const toneColors: Record<Tone, { fg: string; bg: string }> = {
  neutral: { fg: theme.muted, bg: theme.surfaceAlt },
  accent: { fg: theme.accent, bg: theme.accentDim },
  success: { fg: theme.success, bg: theme.successDim },
  warning: { fg: theme.warning, bg: theme.warningDim },
  danger: { fg: theme.danger, bg: theme.dangerDim },
  info: { fg: theme.info, bg: theme.infoDim },
};

export function Badge({
  children,
  tone = 'neutral',
}: {
  children: ReactNode;
  tone?: Tone;
}) {
  const c = toneColors[tone];
  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 4,
        padding: '2px 8px',
        borderRadius: 999,
        fontSize: 12,
        fontWeight: 600,
        color: c.fg,
        background: c.bg,
        whiteSpace: 'nowrap',
      }}
    >
      {children}
    </span>
  );
}

export function Card({
  children,
  active,
  style,
}: {
  children: ReactNode;
  active?: boolean;
  style?: CSSProperties;
}) {
  return (
    <div
      style={{
        background: theme.surface,
        border: `1px solid ${active ? theme.accent : theme.border}`,
        borderRadius: 10,
        padding: '1.25rem',
        ...style,
      }}
    >
      {children}
    </div>
  );
}

export function CardHeader({
  title,
  subtitle,
  right,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  right?: ReactNode;
}) {
  return (
    <div
      style={{
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'flex-start',
        gap: 12,
        marginBottom: 14,
      }}
    >
      <div>
        <div style={{ fontSize: 15, fontWeight: 700 }}>{title}</div>
        {subtitle && (
          <div style={{ fontSize: 12, color: theme.muted, marginTop: 2 }}>
            {subtitle}
          </div>
        )}
      </div>
      {right}
    </div>
  );
}

export function Label({ children }: { children: ReactNode }) {
  return (
    <label
      style={{
        display: 'block',
        fontSize: 12,
        color: theme.muted,
        marginBottom: 4,
      }}
    >
      {children}
    </label>
  );
}

export function Input(props: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      {...props}
      style={{
        width: '100%',
        padding: '8px 12px',
        background: theme.bg,
        border: `1px solid ${theme.border}`,
        color: theme.text,
        borderRadius: 6,
        outline: 'none',
        ...props.style,
      }}
    />
  );
}

export function Select(props: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      {...props}
      style={{
        width: '100%',
        padding: '8px 12px',
        background: theme.bg,
        border: `1px solid ${theme.border}`,
        color: theme.text,
        borderRadius: 6,
        ...props.style,
      }}
    />
  );
}

type ButtonVariant = 'primary' | 'secondary' | 'danger' | 'ghost';

const buttonStyles: Record<ButtonVariant, CSSProperties> = {
  primary: { background: theme.accent, color: '#04121a' },
  secondary: { background: theme.border, color: theme.text },
  danger: { background: theme.danger, color: '#fff' },
  ghost: { background: 'transparent', color: theme.muted },
};

export function Button({
  children,
  variant = 'primary',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant }) {
  return (
    <button
      {...props}
      style={{
        padding: '8px 16px',
        border: 'none',
        borderRadius: 6,
        cursor: props.disabled ? 'not-allowed' : 'pointer',
        fontWeight: 600,
        fontSize: 13,
        opacity: props.disabled ? 0.55 : 1,
        ...buttonStyles[variant],
        ...props.style,
      }}
    >
      {children}
    </button>
  );
}

export function Spinner({ label }: { label?: string }) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 10,
        color: theme.muted,
        fontSize: 14,
      }}
    >
      <span
        style={{
          width: 16,
          height: 16,
          borderRadius: '50%',
          border: `2px solid ${theme.border}`,
          borderTopColor: theme.accent,
          display: 'inline-block',
          animation: 'ab-spin 0.8s linear infinite',
        }}
      />
      {label ?? '加载�?..'}
    </div>
  );
}

export function EmptyState({
  title = '暂无数据',
  hint,
}: {
  title?: string;
  hint?: string;
}) {
  return (
    <div
      style={{
        padding: '1.25rem',
        border: `1px dashed ${theme.border}`,
        borderRadius: 8,
        color: theme.muted,
        fontSize: 13,
        textAlign: 'center',
      }}
    >
      <div style={{ fontWeight: 600, color: theme.text }}>{title}</div>
      {hint && <div style={{ marginTop: 4 }}>{hint}</div>}
    </div>
  );
}

export function ErrorState({
  message,
  onRetry,
}: {
  message: string;
  onRetry?: () => void;
}) {
  return (
    <div
      style={{
        padding: '1rem 1.25rem',
        border: `1px solid ${theme.danger}`,
        background: theme.dangerDim,
        borderRadius: 8,
        color: theme.text,
        fontSize: 13,
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'center',
        gap: 12,
      }}
    >
      <span>加载失败：{message}</span>
      {onRetry && (
        <Button variant="secondary" onClick={onRetry}>
          重试
        </Button>
      )}
    </div>
  );
}

export function InlineError({ message }: { message: string }) {
  return (
    <div style={{ color: theme.danger, fontSize: 12, marginTop: 4 }}>
      {message}
    </div>
  );
}

export function StatCard({
  label,
  value,
  hint,
  tone = 'accent',
}: {
  label: ReactNode;
  value: ReactNode;
  hint?: ReactNode;
  tone?: Tone;
}) {
  return (
    <Card style={{ padding: '1.1rem 1.25rem' }}>
      <div style={{ fontSize: 12, color: theme.muted, marginBottom: 6 }}>
        {label}
      </div>
      <div
        style={{
          fontSize: 26,
          fontWeight: 700,
          color: toneColors[tone].fg,
          lineHeight: 1.15,
          overflowWrap: 'anywhere',
        }}
      >
        {value}
      </div>
      {hint && (
        <div style={{ fontSize: 12, color: theme.faint, marginTop: 6 }}>
          {hint}
        </div>
      )}
    </Card>
  );
}
