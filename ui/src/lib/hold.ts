// A long press, or the right button: the gesture that asks "what can I do
// with this?". On touch it is a finger held still for a moment; with a mouse
// it is the context click. The press that opened the menu never also counts
// as a click.

const HOLD_MS = 450;
const SLOP = 8;

export function hold(node: HTMLElement, onhold: (at: { x: number; y: number }) => void) {
  let timer = 0;
  let from = { x: 0, y: 0 };
  let fired = false;
  let handler = onhold;

  const clear = () => {
    clearTimeout(timer);
    timer = 0;
  };
  const down = (e: PointerEvent) => {
    fired = false;
    // the mouse has a button for this; holding it down is a drag, not a question
    if (e.pointerType === "mouse") return;
    from = { x: e.clientX, y: e.clientY };
    clear();
    timer = window.setTimeout(() => {
      timer = 0;
      fired = true;
      navigator.vibrate?.(8);
      handler(from);
    }, HOLD_MS);
  };
  const move = (e: PointerEvent) => {
    if (timer && Math.hypot(e.clientX - from.x, e.clientY - from.y) > SLOP) clear();
  };
  const context = (e: MouseEvent) => {
    e.preventDefault();
    clear();
    // Android sends this after its own long press: the timer has already answered
    if (fired) return;
    fired = true;
    handler({ x: e.clientX, y: e.clientY });
  };
  // the click that ends a held press belongs to the menu, not to the thing
  const click = (e: MouseEvent) => {
    if (!fired) return;
    fired = false;
    e.preventDefault();
    e.stopImmediatePropagation();
  };

  node.addEventListener("pointerdown", down);
  node.addEventListener("pointermove", move);
  node.addEventListener("pointerup", clear);
  node.addEventListener("pointercancel", clear);
  node.addEventListener("pointerleave", clear);
  node.addEventListener("contextmenu", context);
  node.addEventListener("click", click, true);
  return {
    update(next: (at: { x: number; y: number }) => void) {
      handler = next;
    },
    destroy() {
      clear();
      node.removeEventListener("pointerdown", down);
      node.removeEventListener("pointermove", move);
      node.removeEventListener("pointerup", clear);
      node.removeEventListener("pointercancel", clear);
      node.removeEventListener("pointerleave", clear);
      node.removeEventListener("contextmenu", context);
      node.removeEventListener("click", click, true);
    },
  };
}
