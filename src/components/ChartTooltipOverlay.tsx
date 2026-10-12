import { useLayoutEffect, useRef, type ReactNode } from "react";

const EDGE_GAP = 8;
const POINT_GAP = 12;

/** Recharts portals own visibility; this surface positions content in the viewport. */
export function ChartTooltipOverlay({
  anchor,
  coordinate,
  children,
}: {
  anchor: HTMLElement | null;
  coordinate: { x?: number; y?: number } | undefined;
  children: ReactNode;
}) {
  const surface = useRef<HTMLDivElement>(null);
  const x = coordinate?.x;
  const y = coordinate?.y;

  useLayoutEffect(() => {
    const element = surface.current;
    const viewport = anchor?.ownerDocument.defaultView;
    if (!element || !anchor || !viewport || x === undefined || y === undefined) return;

    const position = () => {
      element.style.maxHeight = `${viewport.innerHeight - EDGE_GAP * 2}px`;
      const bounds = anchor.getBoundingClientRect();
      const width = element.offsetWidth;
      const height = element.offsetHeight;
      const pointX = bounds.left + x;
      const pointY = bounds.top + y;
      const left =
        pointX + POINT_GAP + width <= viewport.innerWidth - EDGE_GAP
          ? pointX + POINT_GAP
          : pointX - POINT_GAP - width;
      const top =
        pointY + POINT_GAP + height <= viewport.innerHeight - EDGE_GAP
          ? pointY + POINT_GAP
          : pointY - POINT_GAP - height;
      element.style.left = `${Math.max(EDGE_GAP, Math.min(left, viewport.innerWidth - width - EDGE_GAP))}px`;
      element.style.top = `${Math.max(EDGE_GAP, Math.min(top, viewport.innerHeight - height - EDGE_GAP))}px`;
      element.style.visibility = "inherit";
    };

    position();
    const observer = new ResizeObserver(position);
    observer.observe(element);
    observer.observe(anchor);
    viewport.addEventListener("resize", position);
    viewport.addEventListener("scroll", position, true);
    return () => {
      observer.disconnect();
      viewport.removeEventListener("resize", position);
      viewport.removeEventListener("scroll", position, true);
    };
  }, [anchor, x, y]);

  return (
    <div
      ref={surface}
      data-chart-tooltip=""
      className="invisible fixed w-72 max-w-[calc(100dvw-1rem)] overflow-y-auto rounded-md border bg-popover p-3 text-xs text-popover-foreground shadow-md"
    >
      {children}
    </div>
  );
}
