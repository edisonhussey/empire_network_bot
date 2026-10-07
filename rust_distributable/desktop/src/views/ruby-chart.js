import { $, element } from "../dom.js";

/// Smallest "nice" whole-number tick step that fits about four gridlines in
/// `spread`. Never below 1: ruby counts are integers, and a fractional step is
/// what made an all-zero chart print 0.25 / 0.5 labels.
function tickStep(spread) {
  const rough = Math.max(1, spread / 4);
  const magnitude = 10 ** Math.floor(Math.log10(rough));
  return [1, 2, 5, 10].map((unit) => unit * magnitude).find((value) => value >= rough);
}

// STYLE: ruby chart redrawn as a single 1px white line over dashed hairline gridlines.
// No area fill, no gradient, no colour. Both vertical bounds follow the
// lifetime data so a large historic total does not flatten recent changes.
// The viewBox is close to the width it is rendered at - the chart lives in the
// right-hand dashboard column - so the axis text stays legible instead of being
// scaled down with the drawing.
export function renderRubyChart(series) {
  const container = $("#ruby-chart");
  if (!series?.length) { container.replaceChildren(element("p", "empty-copy", "No ruby returns recorded yet.")); return; }
  const width = 640; const height = 300; const left = 40; const right = 8; const top = 12; const bottom = 34;
  const minimumValue = Math.min(...series.map((point) => point.value));
  const maximumValue = Math.max(1, ...series.map((point) => point.value));
  const step = tickStep(maximumValue - minimumValue);
  const floor = Math.max(0, Math.floor(minimumValue / step) * step);
  const ceiling = Math.max(floor + step, Math.ceil(maximumValue / step) * step);
  const plotW = width - left - right; const plotH = height - top - bottom;
  const px = (index) => left + index * plotW / Math.max(1, series.length - 1);
  const py = (value) => top + plotH - (value - floor) * plotH / (ceiling - floor);
  const points = series.map((point, index) => `${px(index).toFixed(1)},${py(point.value).toFixed(1)}`).join(" ");
  let grid = "";
  for (let tick = floor; tick <= ceiling; tick += step) {
    grid += `<line class="grid" x1="${left}" y1="${py(tick)}" x2="${width - right}" y2="${py(tick)}"/><text x="${left - 10}" y="${py(tick) + 4}" text-anchor="end">${tick >= 1000 ? `${tick / 1000}k` : tick}</text>`;
  }
  // Only as many date labels as there are distinct points, and never the same
  // text twice: a series recorded within one day would otherwise repeat the
  // same date across the axis.
  const labelCount = Math.min(6, series.length);
  const seenIndexes = new Set();
  const seenText = new Set();
  let axis = "";
  for (let i = 0; i < labelCount; i += 1) {
    const index = labelCount === 1 ? 0 : Math.round(i * (series.length - 1) / (labelCount - 1));
    if (seenIndexes.has(index)) continue;
    seenIndexes.add(index);
    const text = new Date(series[index].at_ms).toLocaleDateString([], { month: "short", day: "numeric" });
    if (seenText.has(text)) continue;
    seenText.add(text);
    const anchor = labelCount === 1 ? "middle" : i === 0 ? "start" : i === labelCount - 1 ? "end" : "middle";
    axis += `<text x="${px(index)}" y="${height - 8}" text-anchor="${anchor}">${text}</text>`;
  }
  // A single sample has no line to draw, so mark the point itself rather than
  // leaving the plot looking empty.
  const marker = series.length === 1
    ? `<circle class="line" cx="${px(0).toFixed(1)}" cy="${py(series[0].value).toFixed(1)}" r="2.5" fill="none"/>`
    : "";
  // Soft fill under the line and a dot on the latest value, as in the reference.
  const lastX = px(series.length - 1).toFixed(1);
  const lastY = py(series[series.length - 1].value).toFixed(1);
  const baseline = py(floor).toFixed(1);
  const area = series.length > 1
    ? `<polygon class="area" points="${px(0).toFixed(1)},${baseline} ${points} ${lastX},${baseline}"/>`
    : "";
  const frame = `<rect class="frame" x="${left}" y="${top}" width="${plotW}" height="${plotH}"/>`;
  const defs = `<defs><linearGradient id="ruby-fill" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".28"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient></defs>`;
  const endDot = `<circle class="end-dot" cx="${lastX}" cy="${lastY}" r="3.5"/>`;
  container.innerHTML = `<svg viewBox="0 0 ${width} ${height}">${defs}${frame}${grid}${axis}${area}<polyline class="line" points="${points}"/>${marker}${endDot}</svg>`;
}
