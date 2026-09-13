import type { ShowcaseExample } from '@casoon/pages-theme/showcase';
import source from '../../examples/site_showcase.rs?raw';

// Every chart is an SVG written by the crate's own renderers: `cargo run --example
// site_showcase` streams synthetic bars (fixed seeds) through the indicators and writes
// examples/site/*.svg. The site embeds those files unchanged, and the Rust code shown next to
// each chart is sliced from the same example file.
const svgs = import.meta.glob<string>('../../examples/site/*.svg', {
  query: '?raw',
  import: 'default',
  eager: true,
});

function chart(name: string): string {
  const svg = svgs[`../../examples/site/${name}.svg`];
  if (!svg) {
    throw new Error(`Missing examples/site/${name}.svg, run: cargo run --example site_showcase`);
  }
  return svg.trim();
}

/** The SVG has a fixed size, so narrow screens scroll it inside a focusable region. */
export function figure(name: string, label: string): string {
  return `<div role="region" tabindex="0" aria-label="${label} (scrollable)" style="max-width:100%;overflow-x:auto;line-height:0;border-radius:10px">${chart(name)}</div>`;
}

function snippet(slug: string): string {
  const marker = `// showcase:start ${slug}\n`;
  const start = source.indexOf(marker);
  const end = source.indexOf('// showcase:end', start);
  if (start < 0 || end < 0) throw new Error(`No showcase section "${slug}" in site_showcase.rs`);
  return source.slice(start + marker.length, end).trimEnd();
}

const catalogue = [
  {
    slug: 'market-regime',
    title: 'Market regime',
    tags: ['classify_regime', 'adx', 'atr', 'render_scene_svg'],
    description:
      'Synthetic bars in four legs: up, sideways, down, sideways. classify_regime runs on every bar with ADX(14) and ATR(14), and each run of equal regimes is shaded: green for bullish expansion, red for bearish expansion, grey for consolidation, amber for transition. The unshaded start is the ADX and ATR warmup. The lower pane shows ADX with the trend threshold of 20.',
  },
  {
    slug: 'composite-signal',
    title: 'Composite signal',
    tags: ['score_indicator', 'aggregate_subscores', 'render_chart_svg'],
    description:
      'MACD, Vortex and Trend Quality are scored on every bar and aggregated with the current regime, on the same four legs as the regime chart. A marker shows where a Buy or Sell trigger starts, labelled with the composite score. Only a ClearToTrade grade produces a trigger, so a direction against the regime stays silent.',
  },
  {
    slug: 'bollinger-bands',
    title: 'Bollinger Bands',
    tags: ['bollinger', 'render_chart_svg'],
    description:
      'Bollinger Bands (20, 2) on a random walk: upper and lower band in blue, the basis in orange. The lines start once the 20-bar warmup is complete.',
  },
  {
    slug: 'supertrend',
    title: 'Supertrend',
    tags: ['supertrend', 'render_chart_svg'],
    description:
      'Supertrend (10, 3) on the same four legs as the regime chart. The stop line is green while the trend is long and red while it is short; a marker shows each flip.',
  },
  {
    slug: 'volume-profile',
    title: 'Volume profile',
    tags: ['volume_profile', 'render_chart_svg'],
    description:
      'Volume profile over the last 70 bars with 30 price bins. The shaded zone is the value area, the orange line the point of control.',
  },
];

export const examples: ShowcaseExample[] = catalogue.map(({ slug, ...meta }) => ({
  slug,
  ...meta,
  file: 'examples/site_showcase.rs',
  input: { code: snippet(slug), lang: 'rust' },
  output: { html: figure(slug, `${meta.title} chart`), kind: 'panel' },
}));
