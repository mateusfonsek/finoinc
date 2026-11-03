type Rect = { top: number; left: number; bottom: number; right: number };

const GAP = 6;
const MARGIN = 8;

/**
 * Where a menu anchored to `trigger` goes, in viewport coordinates (for a
 * `position: fixed` menu portalled to `<body>`). Below the trigger when it
 * fits; otherwise on the roomier side. Opening upward it is anchored by its
 * bottom edge (`bottom`), so it sits against the trigger whatever its real
 * height. Always inside the window horizontally.
 */
export function menuPosition(
  trigger: Rect,
  viewport: { width: number; height: number },
  menu: { width: number; height: number },
): { top: number; left: number } | { bottom: number; left: number } {
  const below = viewport.height - trigger.bottom - GAP;
  const above = trigger.top - GAP;
  const left = Math.max(MARGIN, Math.min(trigger.left, viewport.width - menu.width - MARGIN));
  if (below < menu.height && above > below) {
    return { bottom: viewport.height - trigger.top + GAP, left };
  }
  return { top: trigger.bottom + GAP, left };
}
