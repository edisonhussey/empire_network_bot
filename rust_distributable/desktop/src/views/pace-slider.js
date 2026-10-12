import { element } from "../dom.js";
import { PACE_MAX, PACE_MIN, paceToPosition, positionToPace } from "./dev-math.js";

/// A slim vertical slider for the test pace of the human-paced waits: up makes every wait
/// longer, down shorter, 1 (the design) marked on the track. Bounded below and above; the
/// service keeps the same bounds. Double-click or Home puts it back to 1.
///
/// It owns only the control. `onChange(pace)` is called, once the pointer settles, with the
/// value to send; `set(pace)` moves it to a value the service reported, unless the
/// operator is holding it or has only just moved it.
export function createPaceSlider({ host, onChange }) {
  const root = element("div", "pace");
  root.setAttribute("role", "slider");
  root.tabIndex = 0;
  root.setAttribute("aria-orientation", "vertical");
  root.setAttribute("aria-label", "Wait length scale");
  root.setAttribute("aria-valuemin", String(PACE_MIN));
  root.setAttribute("aria-valuemax", String(PACE_MAX));
  root.title = "Test pace: stretches or shortens every human-paced wait from now on. 1 is the design. Double-click to reset.";
  const top = element("span", "pace-end", "longer");
  const track = element("div", "pace-track");
  const mark = element("i", "pace-one");
  const fill = element("i", "pace-fill");
  const thumb = element("b", "pace-thumb");
  const value = element("output", "pace-value");
  const bottom = element("span", "pace-end", "shorter");
  track.append(mark, fill, thumb);
  root.append(top, track, value, bottom);
  host.append(root);

  let pace = 1;
  let held = false;
  let timer = 0;
  let touchedAt = 0;

  function draw() {
    const position = paceToPosition(pace);
    thumb.style.bottom = `${position * 100}%`;
    fill.style.height = `${position * 100}%`;
    mark.style.bottom = `${paceToPosition(1) * 100}%`;
    value.textContent = `×${pace.toFixed(2)}`;
    root.classList.toggle("changed", pace !== 1);
    root.setAttribute("aria-valuenow", String(pace));
    root.setAttribute("aria-valuetext", `${pace.toFixed(2)} times the design wait`);
  }

  function change(next, immediate = false) {
    pace = next;
    touchedAt = Date.now();
    draw();
    clearTimeout(timer);
    timer = setTimeout(() => onChange(pace), immediate ? 0 : 250);
  }

  function fromPointer(event) {
    const box = track.getBoundingClientRect();
    change(positionToPace(1 - (event.clientY - box.top) / Math.max(1, box.height)));
  }

  root.addEventListener("pointerdown", (event) => {
    held = true;
    root.setPointerCapture(event.pointerId);
    root.classList.add("held");
    fromPointer(event);
  });
  root.addEventListener("pointermove", (event) => { if (held) fromPointer(event); });
  const release = () => { held = false; root.classList.remove("held"); };
  root.addEventListener("pointerup", release);
  root.addEventListener("pointercancel", release);
  root.addEventListener("dblclick", () => change(1, true));
  root.addEventListener("keydown", (event) => {
    const step = { ArrowUp: 0.05, ArrowRight: 0.05, ArrowDown: -0.05, ArrowLeft: -0.05 }[event.key];
    if (step != null) { event.preventDefault(); change(Math.min(PACE_MAX, Math.max(PACE_MIN, Math.round((pace + step) * 100) / 100))); }
    else if (event.key === "Home") { event.preventDefault(); change(1, true); }
  });
  draw();

  return {
    get: () => pace,
    /// A value the service reported; ignored while the operator is holding the control.
    set(next) {
      // A reading taken before the service heard a change would snap the control back.
      if (held || Date.now() - touchedAt < 2500 || !Number.isFinite(next) || next === pace) return;
      pace = next;
      draw();
    },
  };
}
