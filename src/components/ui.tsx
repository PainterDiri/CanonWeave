import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, TextareaHTMLAttributes } from "react";
import * as Dialog from "@radix-ui/react-dialog";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  tone?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md";
  icon?: ReactNode;
};

export function Button({
  tone = "secondary",
  size = "md",
  icon,
  className = "",
  children,
  ...rest
}: ButtonProps) {
  const tones = {
    primary:
      "bg-teal text-paper-raised border-teal hover:bg-teal-deep disabled:bg-teal/35 disabled:border-transparent",
    secondary:
      "bg-paper-raised text-ink border-line-strong hover:border-teal/50 hover:bg-white disabled:opacity-45",
    ghost: "bg-transparent text-ink-soft border-transparent hover:bg-ink/5",
    danger: "bg-coral text-white border-coral hover:brightness-95 disabled:opacity-40",
  };
  const sizes = {
    sm: "h-8 px-3 text-[12px] gap-1.5 rounded-lg",
    md: "h-10 px-4 text-[13px] gap-2 rounded-[10px]",
  };
  return (
    <button
      className={`focus-ring inline-flex items-center justify-center border font-medium transition-colors duration-150 ${tones[tone]} ${sizes[size]} ${className}`}
      {...rest}
    >
      {icon}
      {children}
    </button>
  );
}

export function Input({ className = "", ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={`focus-ring h-10 w-full rounded-[10px] border border-line-strong bg-paper-raised px-3 text-[13px] text-ink placeholder:text-muted/70 focus:border-teal/60 ${className}`}
      {...rest}
    />
  );
}

export function Label({ children, hint }: { children: ReactNode; hint?: string }) {
  return (
    <div className="mb-1.5 flex items-baseline justify-between gap-3">
      <span className="text-[12px] font-medium tracking-wide text-ink-soft">{children}</span>
      {hint ? <span className="text-[11px] text-muted">{hint}</span> : null}
    </div>
  );
}

export function Badge({
  children,
  tone = "neutral",
}: {
  children: ReactNode;
  tone?: "neutral" | "teal" | "coral" | "amber";
}) {
  const tones = {
    neutral: "bg-ink/5 text-ink-soft border-ink/8",
    teal: "bg-teal-soft text-teal border-teal/15",
    coral: "bg-coral-soft text-coral border-coral/20",
    amber: "bg-amber-soft text-amber border-amber/25",
  };
  return (
    <span
      className={`inline-flex h-[22px] items-center rounded-full border px-2 text-[11px] font-medium ${tones[tone]}`}
    >
      {children}
    </span>
  );
}

export function SectionTitle({
  eyebrow,
  title,
  action,
}: {
  eyebrow?: string;
  title: string;
  action?: ReactNode;
}) {
  return (
    <div className="mb-5 flex items-end justify-between gap-4">
      <div className="flex items-baseline gap-3">
        <h1 className="display text-[28px] leading-tight text-ink">{title}</h1>
        {eyebrow ? <span className="text-[11px] tracking-wide text-muted">{eyebrow}</span> : null}
      </div>
      {action}
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="paper-card flex flex-col items-center justify-center px-8 py-16 text-center">
      <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-full border border-line bg-paper text-teal">
        {icon}
      </div>
      <div className="display mb-2 text-[19px] text-ink">{title}</div>
      <p className="mb-6 max-w-md text-[13px] leading-7 text-muted">{description}</p>
      {action}
    </div>
  );
}

export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div>
      <Label hint={hint}>{label}</Label>
      {children}
    </div>
  );
}

export function Textarea({ className = "", ...rest }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return (
    <textarea
      className={`focus-ring w-full resize-y rounded-[10px] border border-line-strong bg-paper-raised px-3 py-2.5 text-[13px] leading-6 text-ink placeholder:text-muted/70 focus:border-teal/60 ${className}`}
      {...rest}
    />
  );
}

export function Modal({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  width = "max-w-[620px]",
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: string;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-ink/25 backdrop-blur-[2px]" />
        <Dialog.Content
          className={`fixed left-1/2 top-1/2 z-50 max-h-[88vh] w-[calc(100vw-64px)] ${width} -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-2xl border border-line bg-paper-raised p-6 shadow-[0_24px_80px_rgba(42,34,24,0.22)] focus:outline-none`}
        >
          <div className="mb-5 flex items-start justify-between gap-4">
            <div>
              <Dialog.Title className="display text-[21px] text-ink">{title}</Dialog.Title>
              {description ? (
                <Dialog.Description className="mt-1.5 text-[12px] leading-5 text-muted">
                  {description}
                </Dialog.Description>
              ) : null}
            </div>
            <Dialog.Close asChild>
              <button
                className="focus-ring flex h-7 w-7 items-center justify-center rounded-full text-muted transition hover:bg-ink/5 hover:text-ink"
                aria-label="关闭"
              >
                ×
              </button>
            </Dialog.Close>
          </div>
          {children}
          {footer ? <div className="mt-6 flex justify-end gap-2">{footer}</div> : null}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
