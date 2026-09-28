import { useEffect, useId, useRef, type ReactNode } from "react";
import { X } from "lucide-react";

interface DialogProps {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  closeLabel: string;
  /** Backdrop click / Esc close the dialog. Turn off while a payment is in flight. */
  dismissible?: boolean;
  children: ReactNode;
}

/**
 * Native modal `<dialog>`: focus trap, inert page and Esc handling come from the platform.
 * Content mounts only while open so its entry animation replays on every open.
 */
export default function Dialog({
  open,
  onClose,
  title,
  closeLabel,
  dismissible = true,
  children,
}: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    else if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      onCancel={(e) => {
        e.preventDefault();
        if (dismissible) onClose();
      }}
      // The platform may still force-close (repeated Esc); keep React state in sync.
      onClose={() => {
        if (open) onClose();
      }}
      onClick={(e) => {
        if (dismissible && e.target === e.currentTarget) onClose();
      }}
      className="m-auto w-[min(440px,calc(100vw-24px))] max-w-none overflow-visible bg-transparent p-0 text-fg backdrop:bg-bg/70 backdrop:backdrop-blur-[6px]"
    >
      {open && (
        <div className="flex max-h-[calc(100dvh-24px)] animate-[palette-in_240ms_cubic-bezier(0.22,1,0.36,1)] flex-col overflow-hidden rounded-[14px] bg-elev shadow-[0_0_0_1px_var(--line-strong),var(--shadow-window)]">
          <div className="flex h-12 shrink-0 items-center justify-between gap-4 border-b border-line pl-5 pr-2">
            <h2 id={titleId} className="truncate font-mono text-[11.5px] uppercase tracking-[0.12em] text-muted">
              {title}
            </h2>
            <button type="button" onClick={onClose} aria-label={closeLabel} className="btn btn-ghost btn-sm w-8 px-0">
              <X size={15} aria-hidden />
            </button>
          </div>
          <div className="overflow-y-auto px-5 py-6 sm:px-6">{children}</div>
        </div>
      )}
    </dialog>
  );
}
