import {
  createElement,
  useEffect,
  useId,
  useRef,
  type ReactNode,
  type FormEvent,
  type ComponentPropsWithRef,
} from "react";
import {
  FileTextIcon,
  FileCodeIcon,
  FilePdfIcon,
  FileXlsIcon,
  FilePptIcon,
  ImageIcon,
  XIcon,
  SpinnerGapIcon,
  CaretDownIcon,
} from "@phosphor-icons/react";

// Keep native select navigation and form behavior. The selected value has its
// own shrinkable box so long project names do not change the control height.
export function Select({
  children,
  ...props
}: ComponentPropsWithRef<"select">) {
  return (
    <select {...props}>
      {!props.multiple && (
        <button type="button" className="select-value">
          {createElement("selectedcontent")}
          <CaretDownIcon className="select-caret" size={16} aria-hidden />
        </button>
      )}
      {children}
    </select>
  );
}

export function FileIcon({ extension }: { extension: string }) {
  if (["pdf"].includes(extension))
    return <FilePdfIcon size={30} weight="regular" aria-hidden />;
  if (["xlsx", "csv", "tsv"].includes(extension))
    return <FileXlsIcon size={30} weight="regular" aria-hidden />;
  if (["pptx"].includes(extension))
    return <FilePptIcon size={30} weight="regular" aria-hidden />;
  if (["png", "jpg", "jpeg", "tiff", "tif"].includes(extension))
    return <ImageIcon size={30} weight="regular" aria-hidden />;
  if (
    [
      "py",
      "js",
      "jsx",
      "ts",
      "tsx",
      "rs",
      "go",
      "java",
      "cpp",
      "c",
      "h",
      "sql",
      "swift",
      "ipynb",
      "json",
    ].includes(extension)
  )
    return <FileCodeIcon size={30} weight="regular" aria-hidden />;
  return <FileTextIcon size={30} weight="regular" aria-hidden />;
}

export function Busy({ children = "处理中…" }: { children?: ReactNode }) {
  return (
    <span className="busy" role="status">
      <SpinnerGapIcon className="spin" size={16} aria-hidden />
      {children}
    </span>
  );
}

export function Empty({
  title,
  children,
  action,
}: {
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="empty">
      <h3>{title}</h3>
      {children && <p>{children}</p>}
      {action && <div className="actions">{action}</div>}
    </div>
  );
}

export function Modal({
  title,
  children,
  onClose,
  onSubmit,
  submitLabel = "保存",
  busy = false,
  submitDisabled = false,
  wide = false,
  cancelLabel = "取消",
  className = "",
  focusHeading = false,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  onSubmit?: (event: FormEvent) => void;
  submitLabel?: string;
  busy?: boolean;
  submitDisabled?: boolean;
  wide?: boolean;
  cancelLabel?: string;
  className?: string;
  focusHeading?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const heading = useId();
  useEffect(() => {
    const dialog = ref.current!;
    dialog.showModal();
    if (focusHeading) dialog.querySelector<HTMLElement>("h2")?.focus();
    return () => dialog.close();
  }, [focusHeading]);
  useEffect(() => {
    const dialog = ref.current!;
    const cancel = (event: Event) => {
      event.preventDefault();
      if (!busy) onClose();
    };
    dialog.addEventListener("cancel", cancel);
    return () => {
      dialog.removeEventListener("cancel", cancel);
    };
  }, [onClose, busy]);
  const content = (
    <>
      <div className="modal-heading">
        <h2 id={heading} tabIndex={focusHeading ? -1 : undefined}>
          {title}
        </h2>
        <button
          type="button"
          className="icon-button"
          aria-label="关闭"
          onClick={onClose}
          disabled={busy}
        >
          <XIcon size={20} />
        </button>
      </div>
      <div className="modal-body">{children}</div>
      {onSubmit && (
        <div className="modal-footer">
          <button type="button" onClick={onClose} disabled={busy}>
            {cancelLabel}
          </button>
          <button
            className="primary"
            type="submit"
            disabled={busy || submitDisabled}
          >
            {busy ? <Busy /> : submitLabel}
          </button>
        </div>
      )}
    </>
  );
  return (
    <dialog
      ref={ref}
      className={
        "modal" +
        (wide ? " modal-wide" : "") +
        (className ? " " + className : "")
      }
      aria-labelledby={heading}
    >
      {onSubmit ? <form onSubmit={onSubmit}>{content}</form> : content}
    </dialog>
  );
}

export function date(value: string | null, includeTime = false) {
  if (!value) return "尚未更新";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.valueOf())) return value;
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "long",
    day: "numeric",
    ...(includeTime
      ? { hour: "2-digit" as const, minute: "2-digit" as const }
      : {}),
  }).format(parsed);
}

export function relativeDate(value: string | null) {
  if (!value) return "等待首次同步";
  const difference = Date.now() - new Date(value).valueOf();
  if (difference < 60000) return "刚刚已同步";
  if (difference < 3600000)
    return Math.floor(difference / 60000) + " 分钟前同步";
  return date(value, true);
}

export function size(value: number) {
  if (value < 1024) return value + " B";
  if (value < 1024 * 1024) return (value / 1024).toFixed(1) + " KB";
  return (value / (1024 * 1024)).toFixed(1) + " MB";
}
