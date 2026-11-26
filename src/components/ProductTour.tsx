import { useLayoutEffect, useRef, useState } from "react";
import { XIcon } from "@phosphor-icons/react";
import { guideSteps, type GuideStatus } from "../guide";

type Rect = { left: number; top: number; width: number; height: number };
type Placement = {
  left: number;
  top: number;
  hole: Rect | null;
  ready: boolean;
  maxHeight?: number;
};
const margin = 16;
const gap = 14;

function place(
  target: Rect | null,
  width: number,
  height: number,
  minimumHeight: number,
): Placement {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const limit = (value: number, low: number, high: number) =>
    Math.max(low, Math.min(value, Math.max(low, high)));
  if (!target)
    return {
      left: (vw - width) / 2,
      top: Math.max(margin, (vh - height) / 2),
      hole: null,
      ready: true,
    };
  const candidates = [
    {
      left: target.left + target.width + gap,
      top: limit(target.top, margin, vh - height - margin),
    },
    {
      left: target.left - width - gap,
      top: limit(target.top, margin, vh - height - margin),
    },
    {
      left: limit(target.left, margin, vw - width - margin),
      top: target.top + target.height + gap,
    },
    {
      left: limit(target.left, margin, vw - width - margin),
      top: target.top - height - gap,
    },
  ];
  const fitting = candidates.find(
    ({ left, top }) =>
      left >= margin &&
      top >= margin &&
      left + width <= vw - margin &&
      top + height <= vh - margin,
  );
  if (fitting) return { ...fitting, hole: target, ready: true };
  // Keep the real target visible in a short/narrow window. Scroll only the
  // explanatory body when the full card cannot fit above or below the target.
  const above = Math.max(0, target.top - gap - margin);
  const below = Math.max(0, vh - target.top - target.height - gap - margin);
  const belowTarget = below >= above;
  const available = Math.max(above, below);
  if (available < minimumHeight)
    return place(null, width, Math.min(height, vh - margin * 2), minimumHeight);
  const capped = Math.min(height, available);
  return {
    left: limit(target.left, margin, vw - width - margin),
    top: belowTarget
      ? target.top + target.height + gap
      : target.top - gap - capped,
    maxHeight: capped,
    hole: target,
    ready: true,
  };
}

export function ProductTour({
  index,
  onStep,
  onExit,
}: {
  index: number;
  onStep: (index: number) => void;
  onExit: (status: GuideStatus) => void;
}) {
  const step = guideSteps[index];
  const dialog = useRef<HTMLDialogElement>(null);
  const card = useRef<HTMLElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const exit = useRef(onExit);
  const [placement, setPlacement] = useState<Placement>({
    left: margin,
    top: margin,
    hole: null,
    ready: false,
  });
  useLayoutEffect(() => {
    exit.current = onExit;
  }, [onExit]);

  useLayoutEffect(() => {
    const element = dialog.current!;
    const root = document.documentElement;
    const previousOverflow = root.style.overflow;
    root.style.overflow = "hidden";
    element.showModal();
    const cancel = (event: Event) => {
      event.preventDefault();
      exit.current("skipped");
    };
    element.addEventListener("cancel", cancel);
    return () => {
      element.removeEventListener("cancel", cancel);
      element.close();
      root.style.overflow = previousOverflow;
    };
  }, []);

  useLayoutEffect(() => {
    setPlacement((previous) => ({
      ...previous,
      ready: false,
      hole: null,
      maxHeight: undefined,
    }));
    if (body.current) body.current.scrollTop = 0;
    let frame = 0;
    let attempts = 0;
    let reveal = true;
    let disposed = false;
    const measure = () => {
      frame = 0;
      if (disposed || !card.current) return;
      const target = document.querySelector<HTMLElement>(step.target);
      const visible = target && target.getClientRects().length > 0;
      if (!visible && attempts++ < 30) {
        frame = requestAnimationFrame(measure);
        return;
      }
      if (visible && reveal) {
        reveal = false;
        const sidebar = target.closest(".primary-sidebar");
        target.scrollIntoView({
          block: sidebar ? "nearest" : "start",
          inline: "nearest",
          behavior: "instant",
        });
        if (!sidebar) window.scrollBy(0, -22);
      }
      const bounds = visible ? target.getBoundingClientRect() : null;
      const hole = bounds
        ? {
            left: Math.max(6, bounds.left - 6),
            top: Math.max(6, bounds.top - 6),
            width: Math.max(
              0,
              Math.min(innerWidth - 6, bounds.right + 6) -
                Math.max(6, bounds.left - 6),
            ),
            height: Math.max(
              0,
              Math.min(innerHeight - 6, bounds.bottom + 6) -
                Math.max(6, bounds.top - 6),
            ),
          }
        : null;
      const size = card.current.getBoundingClientRect();
      // Measure uncapped content rather than alternating between a capped
      // height and its expanded height on successive ResizeObserver callbacks.
      const naturalHeight = Math.ceil(
        size.height +
          Math.max(
            0,
            (body.current?.scrollHeight || 0) -
              (body.current?.clientHeight || 0),
          ),
      );
      const chromeHeight = Math.ceil(
        size.height - (body.current?.clientHeight || 0),
      );
      const minimumHeight =
        chromeHeight + Math.min(body.current?.scrollHeight || 0, 80);
      const next = place(
        hole?.width && hole.height ? hole : null,
        size.width,
        naturalHeight,
        minimumHeight,
      );
      setPlacement((previous) =>
        JSON.stringify(previous) === JSON.stringify(next) ? previous : next,
      );
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(measure);
    };
    const resize = () => {
      reveal = true;
      schedule();
    };
    const observer = new ResizeObserver(schedule);
    observer.observe(card.current!);
    const changes = new MutationObserver((records) => {
      if (
        records.some(
          (record) =>
            !(record.target instanceof Element) ||
            !record.target.closest(".product-tour"),
        )
      )
        schedule();
    });
    changes.observe(document.querySelector(".application")!, {
      childList: true,
      subtree: true,
      attributes: true,
    });
    window.addEventListener("resize", resize);
    window.addEventListener("scroll", schedule, {
      capture: true,
      passive: true,
    });
    window.visualViewport?.addEventListener("resize", resize);
    heading.current?.focus({ preventScroll: true });
    schedule();
    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      observer.disconnect();
      changes.disconnect();
      window.removeEventListener("resize", resize);
      window.removeEventListener("scroll", schedule, true);
      window.visualViewport?.removeEventListener("resize", resize);
    };
  }, [step]);

  useLayoutEffect(() => {
    if (placement.ready) heading.current?.focus({ preventScroll: true });
  }, [index, placement.ready]);

  const last = index === guideSteps.length - 1;
  return (
    <dialog
      ref={dialog}
      className="product-tour"
      aria-labelledby="guide-heading"
      aria-describedby="guide-description"
      onKeyDown={(event) => {
        if (event.altKey || event.ctrlKey || event.metaKey) return;
        if (event.key === "Tab") {
          const controls = Array.from(
            card.current!.querySelectorAll<HTMLElement>(
              'button:not(:disabled), a[href], [tabindex="0"]',
            ),
          ).filter((element) => element.getClientRects().length > 0);
          const first = controls[0];
          const lastControl = controls.at(-1);
          if (
            event.shiftKey &&
            (document.activeElement === first ||
              document.activeElement === heading.current)
          ) {
            event.preventDefault();
            lastControl?.focus();
          } else if (
            !event.shiftKey &&
            document.activeElement === lastControl
          ) {
            event.preventDefault();
            first?.focus();
          }
          return;
        }
        if (event.shiftKey) return;
        if (event.key === "ArrowRight" && !last) {
          event.preventDefault();
          onStep(index + 1);
        }
        if (event.key === "ArrowLeft" && index > 0) {
          event.preventDefault();
          onStep(index - 1);
        }
      }}
    >
      {placement.hole ? (
        <div className="tour-spotlight" aria-hidden style={placement.hole} />
      ) : (
        <div className="tour-dim" aria-hidden />
      )}
      <section
        ref={card}
        className="tour-card"
        data-step={step.id}
        data-positioned={placement.ready}
        style={{
          left: placement.left,
          top: placement.top,
          maxHeight: placement.maxHeight,
        }}
      >
        <div className="tour-card-heading">
          <span className="tour-location">{step.location}</span>
          <button
            type="button"
            className="icon-button"
            aria-label="退出使用指南"
            title="退出使用指南"
            onClick={() => onExit("skipped")}
          >
            <XIcon size={18} aria-hidden />
          </button>
        </div>
        <div ref={body} className="tour-body">
          <h2 id="guide-heading" tabIndex={-1} ref={heading}>
            {step.title}
          </h2>
          <p id="guide-description">{step.description}</p>
          <p className="tour-detail">{step.detail}</p>
        </div>
        <div className="tour-footer">
          <div className="tour-progress" aria-live="polite">
            <span>
              第 {index + 1} 步，共 {guideSteps.length} 步
            </span>
            <span>使用指南</span>
          </div>
          <progress
            value={index + 1}
            max={guideSteps.length}
            aria-label="导览进度"
          />
          <div className="tour-actions">
            <button
              type="button"
              className="text-action"
              onClick={() => onExit("skipped")}
            >
              跳过导览
            </button>
            <div className="tour-actions-right">
              <button
                type="button"
                disabled={index === 0}
                onClick={() => onStep(index - 1)}
              >
                上一步
              </button>
              <button
                type="button"
                className="primary"
                onClick={() => (last ? onExit("completed") : onStep(index + 1))}
              >
                {last ? "完成导览" : "下一步"}
              </button>
            </div>
          </div>
        </div>
      </section>
    </dialog>
  );
}
