import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function generateFigures() {
  const summaryPath = path.resolve(__dirname, 'data/summary_metrics.json');
  const summary = JSON.parse(await readFile(summaryPath, 'utf8'));

  const tasks = Object.entries(summary.task_breakdown);

  // Figure 1: Accuracy Comparison (Grouped Bar Chart)
  // 9 tasks, 2 bars per task
  const width = 800;
  const height = 450;
  const padding = { top: 60, right: 40, bottom: 100, left: 60 };
  const chartWidth = width - padding.left - padding.right;
  const chartHeight = height - padding.top - padding.bottom;

  const barGroupWidth = chartWidth / tasks.length;
  const barWidth = barGroupWidth * 0.35;

  let barsSvg = '';
  let labelsSvg = '';
  let gridSvg = '';

  // Horizontal grid lines (0%, 25%, 50%, 75%, 100%)
  for (let pct = 0; pct <= 100; pct += 25) {
    const y = padding.top + chartHeight - (pct / 100) * chartHeight;
    gridSvg += `<line x1="${padding.left}" y1="${y}" x2="${width - padding.right}" y2="${y}" stroke="#e2e8f0" stroke-width="1" stroke-dasharray="${pct === 0 ? 'none' : '4,4'}" />\n`;
    gridSvg += `<text x="${padding.left - 10}" y="${y + 4}" text-anchor="end" font-size="11" font-family="system-ui, sans-serif" fill="#64748b">${pct}%</text>\n`;
  }

  tasks.forEach(([taskId, task], idx) => {
    const groupX = padding.left + idx * barGroupWidth;
    const xWith = groupX + barGroupWidth * 0.12;
    const xNo = xWith + barWidth + 4;

    const hWith = (task.with_bold.avg_score / 100) * chartHeight;
    const hNo = (task.no_bold.avg_score / 100) * chartHeight;
    const yWith = padding.top + chartHeight - hWith;
    const yNo = padding.top + chartHeight - hNo;

    // With Bold bar (Deep Slate Blue #3b82f6)
    barsSvg += `<rect x="${xWith}" y="${yWith}" width="${barWidth}" height="${hWith}" fill="#3b82f6" rx="2" />\n`;
    // No Bold bar (Emerald Teal #10b981)
    barsSvg += `<rect x="${xNo}" y="${yNo}" width="${barWidth}" height="${hNo}" fill="#10b981" rx="2" />\n`;

    // Values on top of bars if space
    barsSvg += `<text x="${xWith + barWidth / 2}" y="${Math.max(yWith - 5, padding.top + 12)}" text-anchor="middle" font-size="10" font-weight="600" font-family="system-ui, sans-serif" fill="#1e293b">${Math.round(task.with_bold.avg_score)}%</text>\n`;
    barsSvg += `<text x="${xNo + barWidth / 2}" y="${Math.max(yNo - 5, padding.top + 12)}" text-anchor="middle" font-size="10" font-weight="600" font-family="system-ui, sans-serif" fill="#1e293b">${Math.round(task.no_bold.avg_score)}%</text>\n`;

    // Short X label
    const shortName = taskId.replace('task_', 'T').replace('_', ' ').slice(0, 7);
    labelsSvg += `<text x="${groupX + barGroupWidth / 2}" y="${height - padding.bottom + 20}" text-anchor="middle" font-size="11" font-weight="500" font-family="system-ui, sans-serif" fill="#334155">${shortName}</text>\n`;
  });

  const fig1Svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}">
  <rect width="100%" height="100%" fill="#ffffff" />
  <text x="${padding.left}" y="32" font-size="16" font-weight="700" font-family="system-ui, sans-serif" fill="#0f172a">Figure 1: Task Accuracy Comparison: With Bold (**) vs Plain Markdown</text>
  <text x="${padding.left}" y="50" font-size="12" font-family="system-ui, sans-serif" fill="#64748b">Evaluated on MiniMax-M3[1m] across 9 diverse tasks (3 repetitions each)</text>
  
  <!-- Legend -->
  <rect x="${width - padding.right - 230}" y="24" width="14" height="14" fill="#3b82f6" rx="2" />
  <text x="${width - padding.right - 210}" y="36" font-size="12" font-family="system-ui, sans-serif" fill="#1e293b">With Bold (**...**)</text>
  <rect x="${width - padding.right - 100}" y="24" width="14" height="14" fill="#10b981" rx="2" />
  <text x="${width - padding.right - 80}" y="36" font-size="12" font-family="system-ui, sans-serif" fill="#1e293b">Plain (No Bold)</text>

  <!-- Grid & Axes -->
  ${gridSvg}
  ${barsSvg}
  ${labelsSvg}
</svg>`;

  // Figure 2: Input Token Reduction (Prompt Efficiency)
  let fig2BarsSvg = '';
  let fig2LabelsSvg = '';
  let fig2GridSvg = '';

  const maxTokens = Math.max(...tasks.map(([, t]) => t.with_bold.avg_input_tokens)) * 1.15;

  for (let tStep = 0; tStep <= maxTokens; tStep += 100) {
    const y = padding.top + chartHeight - (tStep / maxTokens) * chartHeight;
    fig2GridSvg += `<line x1="${padding.left}" y1="${y}" x2="${width - padding.right}" y2="${y}" stroke="#e2e8f0" stroke-width="1" stroke-dasharray="${tStep === 0 ? 'none' : '4,4'}" />\n`;
    fig2GridSvg += `<text x="${padding.left - 10}" y="${y + 4}" text-anchor="end" font-size="11" font-family="system-ui, sans-serif" fill="#64748b">${tStep}</text>\n`;
  }

  tasks.forEach(([taskId, task], idx) => {
    const groupX = padding.left + idx * barGroupWidth;
    const xWith = groupX + barGroupWidth * 0.12;
    const xNo = xWith + barWidth + 4;

    const hWith = (task.with_bold.avg_input_tokens / maxTokens) * chartHeight;
    const hNo = (task.no_bold.avg_input_tokens / maxTokens) * chartHeight;
    const yWith = padding.top + chartHeight - hWith;
    const yNo = padding.top + chartHeight - hNo;

    fig2BarsSvg += `<rect x="${xWith}" y="${yWith}" width="${barWidth}" height="${hWith}" fill="#64748b" rx="2" />\n`;
    fig2BarsSvg += `<rect x="${xNo}" y="${yNo}" width="${barWidth}" height="${hNo}" fill="#0ea5e9" rx="2" />\n`;

    // Savings tag on top
    const saved = task.token_savings_per_call;
    fig2BarsSvg += `<text x="${xNo + barWidth / 2}" y="${Math.max(yNo - 6, padding.top + 12)}" text-anchor="middle" font-size="10" font-weight="700" font-family="system-ui, sans-serif" fill="#0369a1">-${saved}</text>\n`;

    const shortName = taskId.replace('task_', 'T').replace('_', ' ').slice(0, 7);
    fig2LabelsSvg += `<text x="${groupX + barGroupWidth / 2}" y="${height - padding.bottom + 20}" text-anchor="middle" font-size="11" font-weight="500" font-family="system-ui, sans-serif" fill="#334155">${shortName}</text>\n`;
  });

  const fig2Svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}">
  <rect width="100%" height="100%" fill="#ffffff" />
  <text x="${padding.left}" y="32" font-size="16" font-weight="700" font-family="system-ui, sans-serif" fill="#0f172a">Figure 2: Input Token Consumption &amp; Overhead Savings by Task</text>
  <text x="${padding.left}" y="50" font-size="12" font-family="system-ui, sans-serif" fill="#64748b">Overhead from asterisks (**...**) highlighted with per-call token reduction</text>
  
  <!-- Legend -->
  <rect x="${width - padding.right - 230}" y="24" width="14" height="14" fill="#64748b" rx="2" />
  <text x="${width - padding.right - 210}" y="36" font-size="12" font-family="system-ui, sans-serif" fill="#1e293b">With Bold Tokens</text>
  <rect x="${width - padding.right - 100}" y="24" width="14" height="14" fill="#0ea5e9" rx="2" />
  <text x="${width - padding.right - 80}" y="36" font-size="12" font-family="system-ui, sans-serif" fill="#1e293b">Plain Tokens</text>

  <!-- Grid & Axes -->
  ${fig2GridSvg}
  ${fig2BarsSvg}
  ${fig2LabelsSvg}
</svg>`;

  const fig1Path = path.resolve(__dirname, 'figures/fig1_accuracy_comparison.svg');
  const fig2Path = path.resolve(__dirname, 'figures/fig2_token_savings.svg');

  await writeFile(fig1Path, fig1Svg, 'utf8');
  await writeFile(fig2Path, fig2Svg, 'utf8');

  console.log(`Generated SVG figures:\n - ${fig1Path}\n - ${fig2Path}`);
}

generateFigures().catch(console.error);
