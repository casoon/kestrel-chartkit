import type { ShowcaseExample } from '@casoon/pages-theme/showcase';
import { url } from '@casoon/pages-theme/url';
import { indicators, indicatorSpec, inlineChart, spec } from './chartlet';

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

const perIndicator: ShowcaseExample[] = indicators
  .filter((entry) => entry.chart)
  .map((entry) => ({
    slug: entry.name,
    title: entry.name,
    description: short(entry.description),
    file: FILE,
    group: entry.family,
    tags: [entry.family, `unit: ${entry.unit}`],
    input: { lang: 'rust', code: `// ${entry.description}\n${rustFor(entry.name, entry.defaults)}` },
    output: { html: lazyChart(entry.name), kind: 'panel' },
    preview: { html: lazyChart(entry.name) },
  }));

export const examples: ShowcaseExample[] = [...features, ...perIndicator];
