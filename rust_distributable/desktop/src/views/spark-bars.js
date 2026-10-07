import { element } from "../dom.js";

/// Small column chart: one bar per entry, scaled to the largest value in the
/// series. Zero entries keep a 1px stub so a quiet hour still reads as an hour.
export function renderSparkBars(container, values) {
  const series = values?.length ? values : [];
  const peak = Math.max(1, ...series);
  container.replaceChildren(...series.map((value) => {
    const bar = element("i");
    bar.style.height = `${value > 0 ? Math.max(8, Math.round((value / peak) * 100)) : 0}%`;
    if (value === 0) bar.classList.add("empty");
    return bar;
  }));
}
