import type { ShowcaseExample } from '@casoon/pages-theme/showcase';
import { url } from '@casoon/pages-theme/url';
import { indicators, indicatorSpec, inlineChart, spec, type IndicatorEntry } from './chartlet';

// The showcase is the visual indicator reference: every catalog indicator on the same fictitious
// CASOON share (invented prices), with its default parameters, drawn by chartlet from numbers
// kestrel-chartkit computed. `cargo run --example site_showcase` writes the specifications.

const FILE = 'examples/site_showcase.rs';

const rustFor = (name: string, defaults: Record<string, number>) => {
  const entries = Object.entries(defaults);
  const map = entries.length
    ? `HashMap::from([\n${entries.map(([k, v]) => `    ("${k}".to_string(), ${Number.isInteger(v) ? v.toFixed(1) : v}),`).join('\n')}\n])`
    : 'HashMap::new()';
  return `// Defaults of "${name}" (see the indicator reference)\nlet params = ${map};\nlet mut indicator = build_checked("${name}", &params)?;\n\nfor bar in &bars {\n    if let Some(output) = indicator.on_bar(bar) {\n        // output.value, output.extra[...], indicator.alerts()\n    }\n}`;
};

/** Two lazily loaded files, CSS shows the one matching the page (styles: site/src/pages/…). */
function lazyChart(name: string): string {
  const s = indicatorSpec(name);
  const alt = (s.description ?? '').replace(/"/g, '&quot;');
  const img = (theme: string) =>
    `<img class="ind-${theme}" src="${url(`charts/${name}-${theme}.svg`)}" alt="${alt}" width="${s.width}" height="${s.height}" loading="lazy" decoding="async" style="display:block;max-width:100%;height:auto;border-radius:10px">`;
  const style =
    '<style>.ind-dark{display:none!important}:root[data-theme=dark] .ind-light{display:none!important}:root[data-theme=dark] .ind-dark{display:block!important}@media (prefers-color-scheme:dark){:root:not([data-theme=light]) .ind-light{display:none!important}:root:not([data-theme=light]) .ind-dark{display:block!important}}</style>';
  return `${style}${img('light')}${img('dark')}`;
}

/** The catalog description up to its first parenthesis or sentence end, for a card. */
const short = (text: string) => {
  const cut = text.split(/ \(|\. /)[0];
  return cut.length > 110 ? `${cut.slice(0, 107)}…` : cut;
};

const SIGNALS = 'Regime and signals';

const features: ShowcaseExample[] = [
  {
    slug: 'market-regime',
    group: SIGNALS,
    title: 'Market regime',
    description: String(spec('regime').description),
    file: FILE,
    tags: ['classify_regime', 'adx', 'atr'],
    input: {
      lang: 'rust',
      code: `let mut adx = build_checked("adx", &HashMap::new())?;\nlet mut atr = build_checked("atr", &HashMap::new())?;\n\nfor (i, bar) in bars.iter().enumerate() {\n    let (Some(a), Some(t)) = (adx.on_bar(bar), atr.on_bar(bar)) else { continue };\n    // The ATR indicator reports 100 * ATR / close; classify_regime expects the fraction.\n    let regime = classify_regime(&bars[..=i], a.value, t.value / 100.0);\n}`,
    },
    output: { html: inlineChart(spec('regime'), 'regime'), kind: 'panel' },
  },
  {
    slug: 'composite-signal',
    group: SIGNALS,
    title: 'Composite signal',
    description: String(spec('composite').description),
    file: FILE,
    tags: ['score_indicator', 'aggregate_subscores', 'classify_regime'],
    input: {
      lang: 'rust',
      code: `let subscores = vec![\n    score_indicator("macd", &macd_out, &macd.alerts()),\n    score_indicator("vortex", &vortex_out, &vortex.alerts()),\n    score_indicator("trend_quality", &tq_out, &tq.alerts()),\n];\nlet regime = classify_regime(&bars[..=i], adx_out.value, atr_out.value / 100.0);\nlet signal = aggregate_subscores(subscores, None, regime, Vec::new(), Some(bar), atr_out.extra["raw"]);\n// signal.trigger, signal.score, signal.permission`,
    },
    output: { html: inlineChart(spec('composite'), 'composite'), kind: 'panel' },
  },
];

const escapeHtml = (text: string) =>
  text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

// The mark of an entry that has not passed its acceptance yet, in the theme's own colours
// (text on the surface, so it keeps its contrast in both themes).
const MARK_STYLE =
  '<style>.ind-mark{display:inline-block;margin:0 0 8px;padding:3px 9px;border:1px solid var(--border-strong);border-radius:999px;background:var(--surface);color:var(--text);font-size:12.5px;font-weight:600;line-height:1.3}.ind-note{margin:0 0 14px;padding:10px 14px;border:1px solid var(--border-strong);border-radius:10px;background:var(--surface);color:var(--text);font-size:14.5px;line-height:1.5}.ind-note strong{color:var(--heading)}</style>';
const mark = (entry: IndicatorEntry) =>
  entry.verified ? '' : `<span class="ind-mark">not yet verified</span><br>`;
const notice = (entry: IndicatorEntry) =>
  entry.verified
    ? ''
    : `<p class="ind-note"><strong>Not yet verified:</strong> ${escapeHtml(entry.reason)}.</p>`;

/** Why an entry of another input kind has no chart on this showcase. */
const WHY_NOT_DRAWN: Record<string, string> = {
  Pair: 'It reads two series, the bars and a reference series, and the demo is one share; it is not drawn here.',
  Transform: 'It derives a candle from each bar instead of a value, so it has no pane of its own; it is not drawn here.',
  Intrabar: 'It reads the lower-timeframe bars inside each parent bar, which the demo series does not carry; it is not drawn here.',
};

const otherRust = (entry: IndicatorEntry) => {
  const map = Object.keys(entry.defaults).length
    ? '&params'
    : '&HashMap::new()';
  switch (entry.input) {
    case 'Pair':
      return `let mut indicator = build_pair_checked("${entry.name}", ${map})?;\n// The bars of the two series are paired by timestamp.\nlet outputs = run_pair_batch(indicator.as_mut(), &bars, &reference);`;
    case 'Transform':
      return `let mut transform = build_transform_checked("${entry.name}", ${map})?;\n\nfor bar in &bars {\n    let candle = transform.update(bar);\n}`;
    default:
      return `let mut indicator = build_intrabar_checked("${entry.name}", ${map})?;\n\n// IntrabarGrouper collects the lower-timeframe bars of each parent bar.\nlet delta = indicator.on_group(&group);`;
  }
};

const perIndicator: ShowcaseExample[] = indicators.map((entry) => {
  const base = {
    slug: entry.name,
    title: entry.name,
    description: short(entry.description),
    file: FILE,
    group: entry.family,
    tags: [entry.family, `unit: ${entry.unit}`],
  };
  if (entry.chart) {
    return {
      ...base,
      input: { lang: 'rust', code: `// ${entry.description}\n${rustFor(entry.name, entry.defaults)}` },
      output: { html: `${MARK_STYLE}${notice(entry)}${lazyChart(entry.name)}`, kind: 'panel' as const },
      preview: { html: `${MARK_STYLE}${mark(entry)}${lazyChart(entry.name)}` },
    };
  }
  const why = escapeHtml(WHY_NOT_DRAWN[entry.input]);
  return {
    ...base,
    input: { lang: 'rust', code: `// ${entry.description}\n${otherRust(entry)}` },
    output: { html: `${MARK_STYLE}${notice(entry)}<p class="ind-note">${why}</p>`, kind: 'panel' as const },
    preview: { html: `${MARK_STYLE}${mark(entry)}<span style="display:block;color:var(--muted);font-size:13.5px;line-height:1.45">${why}</span>` },
  };
});

export const examples: ShowcaseExample[] = [...features, ...perIndicator];
