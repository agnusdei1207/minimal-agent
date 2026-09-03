// Minimal-text figure generator for the unified prompt-engineering study.
//
// Design rule (draw-figures skill F1/F2): the image carries structure and values
// only. No chart title, no explanatory sentence, no stat-laden legend, no
// annotation box inside the figure. Titles, conditions and insights live in the
// paper caption. Legends use short names. Value labels appear only where they
// carry signal, never on every identical bar.
//
// Numbers are read live from the experiments' result JSON, so re-running the
// benchmarks and re-running this script keeps the figures in sync with the data.
//
// Pipeline: build one inline-SVG per figure -> wrap in a bare HTML at exact px
// size -> capture with headless Chrome at device-scale 3 -> high-res PNG.

import { readFile, writeFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const OUT = path.join(__dirname, 'figures');
const TMP = path.join(__dirname, '.fig-tmp');
const EXP = path.join(__dirname, '..', '..', 'experiments');
const readJSON = async (p) => JSON.parse(await readFile(p, 'utf8'));

// ---- load measured data ----
const infoloss = await readJSON(path.join(EXP, 'information-loss', 'results.json'));
const stressSummary = await readJSON(path.join(EXP, 'prompt-delimiter-benchmark', 'data', 'stress_summary_metrics.json'));
const posSummary = await readJSON(path.join(EXP, 'prompt-delimiter-benchmark', 'data', 'position_summary_metrics.json'));
const bold = await readJSON(path.join(EXP, 'markdown-bold-ablation', 'data', 'summary_metrics.json'));

const P_KEYS = ['P1_NaiveSummary', 'P2_StructuredKV', 'P3_PureJSON', 'P4_TwoStageCoT', 'P5_NoiseStripper', 'P6_ArtifactPointer'];
const DELIMS = ['xml', 'markdown', 'square_bracket', 'plain_colon', 'json'];
const T1_KEY = 'Stress T1 (Injection Escape)';
const BOLD_TASKS = ['task_01_multi_constraint', 'task_02_5_constraint_conflict', 'task_03_context_injection', 'task_04_delimiter_escape_stress', 'task_05_schema_extraction', 'task_06_hierarchical_binding', 'task_07_needle_head', 'task_08_needle_middle', 'task_09_needle_tail'];
const num = (x) => Number(x);

// Low-saturation academic palette; a signal keeps one color across figures.
const C = {
  blue: '#3b82f6', red: '#ef4444', green: '#10b981', amber: '#f59e0b',
  violet: '#8b5cf6', cyan: '#06b6d4', slate: '#64748b',
  ink: '#0f172a', axis: '#475569', grid: '#e2e8f0', mute: '#64748b',
};
const FONT = 'Arial, "Helvetica Neue", system-ui, sans-serif';

const W = 920, H = 690; // ~4:3 canvas
const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

function svgOpen() {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}" font-family='${FONT}'>` +
    `<rect x="0" y="0" width="${W}" height="${H}" fill="#ffffff"/>`;
}
function text(x, y, s, { size = 20, anchor = 'middle', fill = C.ink, weight = 400 } = {}) {
  return `<text x="${x}" y="${y}" font-size="${size}" text-anchor="${anchor}" fill="${fill}" font-weight="${weight}">${esc(s)}</text>`;
}
function yAxis(m, plot, { max, ticks = [0, 25, 50, 75, 100], unit = '' }) {
  let s = '';
  for (const t of ticks) {
    const y = m.top + plot.h - (t / max) * plot.h;
    s += `<line x1="${m.left}" y1="${y}" x2="${m.left + plot.w}" y2="${y}" stroke="${C.grid}" stroke-width="1.2" ${t === 0 ? '' : 'stroke-dasharray="5,5"'}/>`;
    s += text(m.left - 12, y + 6, `${t}${unit}`, { size: 16, anchor: 'end', fill: C.mute });
  }
  return s;
}

// Short horizontal legend drawn in a reserved band (never over the plot).
function legend(cx, y, items, { size = 18 } = {}) {
  const gap = 26, sw = 26;
  const widths = items.map((it) => sw + 8 + it.label.length * size * 0.56);
  const total = widths.reduce((a, b) => a + b, 0) + gap * (items.length - 1);
  let x = cx - total / 2, s = '';
  items.forEach((it, i) => {
    if (it.line) {
      s += `<line x1="${x}" y1="${y}" x2="${x + sw}" y2="${y}" stroke="${it.color}" stroke-width="4"/>`;
      s += `<circle cx="${x + sw / 2}" cy="${y}" r="5" fill="${it.color}"/>`;
    } else {
      s += `<rect x="${x}" y="${y - 9}" width="${sw}" height="18" rx="3" fill="${it.color}"/>`;
    }
    s += text(x + sw + 8, y + 6, it.label, { size, anchor: 'start', fill: C.axis });
    x += widths[i] + gap;
  });
  return s;
}

// ---- Figure 1: multi-hop fact preservation vs exploit success (grouped bars).
function fig1() {
  const cats = ['P1', 'P2', 'P3', 'P4', 'P5', 'P6'];
  const a = infoloss.summary.aggregated_metrics;
  const preservation = P_KEYS.map(k => num(a[k].avg_preservation_rate));
  const exploit = P_KEYS.map(k => num(a[k].exploitability_rate));
  const m = { left: 92, right: 24, top: 70, bottom: 78 };
  const plot = { w: W - m.left - m.right, h: H - m.top - m.bottom };
  const max = 100, band = plot.w / cats.length, bw = band * 0.30;
  let s = svgOpen();
  s += legend(W / 2, 34, [
    { color: C.blue, label: 'Fact preservation' },
    { color: C.red, label: 'Exploit success' },
  ]);
  s += yAxis(m, plot, { max, unit: '%' });
  cats.forEach((c, i) => {
    const gx = m.left + i * band + band / 2;
    const pairs = [[preservation[i], C.blue, -1], [exploit[i], C.red, 1]];
    for (const [v, col, dir] of pairs) {
      const bx = gx + dir * (bw / 2 + 2) - bw / 2;
      const bh = (v / max) * plot.h, by = m.top + plot.h - bh;
      s += `<rect x="${bx}" y="${by}" width="${bw}" height="${bh}" fill="${col}"/>`;
      s += text(bx + bw / 2, by - 8, v.toFixed(0), { size: 15, fill: col, weight: 700 });
    }
    s += text(gx, m.top + plot.h + 26, c, { size: 19, weight: 600 });
  });
  s += text(24, m.top + plot.h / 2, 'Accuracy (%)', { size: 18, anchor: 'middle', fill: C.axis, weight: 600 }).replace('<text', `<text transform="rotate(-90 24 ${m.top + plot.h / 2})"`);
  s += text(m.left + plot.w / 2, H - 20, 'Relay strategy', { size: 18, fill: C.axis, weight: 600 });
  return s + '</svg>';
}

// ---- Figure 2: delimiter robustness. Baseline is 100% for all (stated in the
// caption), so only the two informative series are drawn: stress avg and the T1
// escape drop. Value labels only where the value is below 100.
function fig2() {
  const cats = ['XML', 'Markdown', 'Bracket', 'Colon', 'JSON'];
  const stress = DELIMS.map(d => num(stressSummary[d].overall_mean_score));
  const t1 = DELIMS.map(d => num(stressSummary[d].task_scores[T1_KEY]));
  const m = { left: 92, right: 24, top: 70, bottom: 82 };
  const plot = { w: W - m.left - m.right, h: H - m.top - m.bottom };
  const max = 100, band = plot.w / cats.length, bw = band * 0.30;
  let s = svgOpen();
  s += legend(W / 2, 34, [
    { color: C.amber, label: 'Stress average' },
    { color: C.red, label: 'T1 injection escape' },
  ]);
  s += yAxis(m, plot, { max, unit: '%' });
  cats.forEach((c, i) => {
    const gx = m.left + i * band + band / 2;
    const pairs = [[stress[i], C.amber, -1], [t1[i], C.red, 1]];
    for (const [v, col, dir] of pairs) {
      const bx = gx + dir * (bw / 2 + 2) - bw / 2;
      const bh = (v / max) * plot.h, by = m.top + plot.h - bh;
      s += `<rect x="${bx}" y="${by}" width="${bw}" height="${bh}" fill="${col}"/>`;
      if (v < 99.9) s += text(bx + bw / 2, by - 8, v.toFixed(1), { size: 15, fill: col, weight: 700 });
    }
    s += text(gx, m.top + plot.h + 26, c, { size: 19, weight: 600 });
  });
  s += text(24, m.top + plot.h / 2, 'Compliance (%)', { size: 18, fill: C.axis, weight: 600 }).replace('<text', `<text transform="rotate(-90 24 ${m.top + plot.h / 2})"`);
  s += text(m.left + plot.w / 2, H - 20, 'Delimiter syntax', { size: 18, fill: C.axis, weight: 600 });
  return s + '</svg>';
}

// ---- Figure 3: position sensitivity line chart. No annotation box (caption
// carries it). Short legend in the top band. Only a sub-100 collapse gets a
// label at its marker.
function fig3() {
  const pos = ['Head', 'Middle', 'Tail'];
  const posKeys = ['head', 'middle', 'tail'];
  const styleFor = {
    xml: { name: 'XML', color: C.red, w: 4.5 },
    markdown: { name: 'Markdown', color: C.green, w: 3.5 },
    plain_colon: { name: 'Colon', color: C.cyan, w: 2.6, dash: '2,6' },
    square_bracket: { name: 'Bracket', color: C.amber, w: 2.6, dash: '9,6' },
    json: { name: 'JSON', color: C.violet, w: 2.6, dash: '1,7' },
  };
  const order = ['markdown', 'plain_colon', 'square_bracket', 'json', 'xml'];
  const series = order.map(d => ({ ...styleFor[d], v: posKeys.map(p => num(posSummary[p][d])) }));
  const m = { left: 92, right: 58, top: 66, bottom: 70 };
  const plot = { w: W - m.left - m.right, h: H - m.top - m.bottom };
  const max = 100;
  const xAt = (i) => m.left + (plot.w / (pos.length - 1)) * i;
  const yAt = (v) => m.top + plot.h - (v / max) * plot.h;
  let s = svgOpen();
  s += legend(W / 2, 32, order.map(d => ({ line: true, color: styleFor[d].color, label: styleFor[d].name })), { size: 17 });
  s += yAxis(m, plot, { max, unit: '%' });
  pos.forEach((p, i) => {
    const anchor = i === 0 ? 'start' : i === pos.length - 1 ? 'end' : 'middle';
    s += text(xAt(i), m.top + plot.h + 28, `${p} (${['10%', '50%', '90%'][i]})`, { size: 18, weight: 600, anchor });
  });
  for (const se of series) {
    const pts = se.v.map((v, i) => `${xAt(i)},${yAt(v)}`).join(' ');
    s += `<polyline points="${pts}" fill="none" stroke="${se.color}" stroke-width="${se.w}" ${se.dash ? `stroke-dasharray="${se.dash}"` : ''} stroke-linejoin="round"/>`;
    se.v.forEach((v, i) => s += `<circle cx="${xAt(i)}" cy="${yAt(v)}" r="${se.w + 1.5}" fill="${se.color}"/>`);
  }
  // Label the lowest sub-100 point (the strongest collapse signal), offset into
  // white space to the right of the marker.
  let lowest = null;
  for (const se of series) se.v.forEach((v, i) => { if (v < 99.9 && (!lowest || v < lowest.v)) lowest = { v, i, color: se.color }; });
  if (lowest) s += text(xAt(lowest.i) + 26, yAt(lowest.v) + 6, `${lowest.v.toFixed(0)}%`, { size: 18, fill: lowest.color, weight: 700, anchor: 'start' });
  s += text(24, m.top + plot.h / 2, 'Extraction accuracy (%)', { size: 18, fill: C.axis, weight: 600 }).replace('<text', `<text transform="rotate(-90 24 ${m.top + plot.h / 2})"`);
  s += text(m.left + plot.w / 2, H - 18, 'Target token insertion position', { size: 18, fill: C.axis, weight: 600 });
  return s + '</svg>';
}

// ---- Figure 4: bold ablation. Two bars per task; value labels dropped except
// where the two conditions differ or a score is below 100.
function fig4() {
  const cats = ['T1', 'T2', 'T3', 'T4', 'T5', 'T6', 'T7', 'T8', 'T9'];
  const tb = bold.task_breakdown;
  const withB = BOLD_TASKS.map(t => num(tb[t].with_bold.avg_score));
  const noB = BOLD_TASKS.map(t => num(tb[t].no_bold.avg_score));
  const m = { left: 92, right: 24, top: 70, bottom: 78 };
  const plot = { w: W - m.left - m.right, h: H - m.top - m.bottom };
  const max = 100, band = plot.w / cats.length, bw = band * 0.30;
  let s = svgOpen();
  s += legend(W / 2, 34, [
    { color: C.blue, label: 'With bold' },
    { color: C.green, label: 'No bold' },
  ]);
  s += yAxis(m, plot, { max, unit: '%' });
  cats.forEach((c, i) => {
    const gx = m.left + i * band + band / 2;
    const pairs = [[withB[i], C.blue, -1], [noB[i], C.green, 1]];
    for (const [v, col, dir] of pairs) {
      const bx = gx + dir * (bw / 2 + 2) - bw / 2;
      const bh = (v / max) * plot.h, by = m.top + plot.h - bh;
      s += `<rect x="${bx}" y="${by}" width="${bw}" height="${bh}" fill="${col}"/>`;
    }
    const minv = Math.min(withB[i], noB[i]);
    if (minv < 99.9) s += text(gx, m.top + plot.h - (minv / max) * plot.h - 10, minv.toFixed(1), { size: 15, fill: C.ink, weight: 700 });
    s += text(gx, m.top + plot.h + 26, c, { size: 19, weight: 600 });
  });
  s += text(24, m.top + plot.h / 2, 'Adherence (%)', { size: 18, fill: C.axis, weight: 600 }).replace('<text', `<text transform="rotate(-90 24 ${m.top + plot.h / 2})"`);
  s += text(m.left + plot.w / 2, H - 20, 'Task', { size: 18, fill: C.axis, weight: 600 });
  return s + '</svg>';
}

// ---- Figure 5: token savings per call (single series bar).
function fig5() {
  const cats = ['T1', 'T2', 'T3', 'T4', 'T5', 'T6', 'T7', 'T8', 'T9'];
  const tb = bold.task_breakdown;
  const saved = BOLD_TASKS.map(t => num(tb[t].token_savings_per_call));
  const maxData = Math.max(2, ...saved.map(Math.abs));
  const max = Math.ceil(maxData / 2) * 2;
  const ticks = []; for (let t = 0; t <= max; t += Math.max(1, Math.round(max / 5))) ticks.push(t);
  const m = { left: 80, right: 24, top: 54, bottom: 78 };
  const plot = { w: W - m.left - m.right, h: H - m.top - m.bottom };
  const band = plot.w / cats.length, bw = band * 0.5;
  let s = svgOpen();
  s += yAxis(m, plot, { max, ticks, unit: '' });
  cats.forEach((c, i) => {
    const gx = m.left + i * band + band / 2;
    const v = saved[i], bh = (Math.max(0, v) / max) * plot.h, by = m.top + plot.h - bh;
    s += `<rect x="${gx - bw / 2}" y="${by}" width="${bw}" height="${bh}" fill="${C.cyan}"/>`;
    s += text(gx, by - 8, String(v), { size: 16, fill: C.axis, weight: 700 });
    s += text(gx, m.top + plot.h + 26, c, { size: 19, weight: 600 });
  });
  s += text(18, m.top + plot.h / 2, 'Tokens saved per call', { size: 18, fill: C.axis, weight: 600 }).replace('<text', `<text transform="rotate(-90 18 ${m.top + plot.h / 2})"`);
  s += text(m.left + plot.w / 2, H - 20, 'Task', { size: 18, fill: C.axis, weight: 600 });
  return s + '</svg>';
}

const FIGS = [
  ['fig1_multihop_preservation_and_exploit', fig1],
  ['fig2_delimiter_stress_robustness', fig2],
  ['fig3_position_sensitivity_lost_in_middle', fig3],
  ['fig4_bold_ablation_accuracy', fig4],
  ['fig5_token_consumption_and_savings', fig5],
];

await mkdir(OUT, { recursive: true });
await mkdir(TMP, { recursive: true });
for (const [name, fn] of FIGS) {
  const html = `<!doctype html><meta charset="utf-8"><style>html,body{margin:0;padding:0;background:#fff}</style>${fn()}`;
  await writeFile(path.join(TMP, `${name}.html`), html, 'utf8');
}
console.log(`Wrote ${FIGS.length} SVG/HTML files to ${TMP}`);
