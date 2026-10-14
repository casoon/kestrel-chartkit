import { renderChart } from '@casoon/chartlet';

type Spec = Record<string, unknown>;

/**
 * chartlet specifications written by `cargo run --example site_showcase` (examples/site/*.json):
 * every number in them is computed by kestrel-chartkit, chartlet only draws.
 */
const specs = import.meta.glob<Spec>('../../examples/site/*.json', {
  import: 'default',
  eager: true,
});

/**
 * Each chart is rendered twice, with chartlet's light and dark theme, and CSS shows the one that
 * matches the page: overriding a few colour variables is not enough, the candles and annotation
 * colours come from the theme.
 */
const SURFACE_STYLE = `<style>.cl-surface{border-radius:10px;line-height:0}.cl-surface svg{max-width:100%;height:auto}.cl-light{background:#fff}.cl-dark{display:none;background:#0e131c}:root[data-theme=dark] .cl-light{display:none}:root[data-theme=dark] .cl-dark{display:block}@media (prefers-color-scheme:dark){:root:not([data-theme=light]) .cl-light{display:none}:root:not([data-theme=light]) .cl-dark{display:block}}</style>`;

/** The chart from examples/site/<name>.json as SVG, light and dark. */
export function chartletChart(name: string): string {
  const spec = specs[`../../examples/site/${name}.json`];
  if (!spec) throw new Error(`Missing examples/site/${name}.json, run: cargo run --example site_showcase`);
  const variant = (theme: 'light' | 'dark') => {
    const { content, warnings } = renderChart(
      { ...spec, theme },
      { format: 'svg', idPrefix: `${name}-${theme}` },
    );
    for (const warning of warnings) console.warn(`[chartlet ${name}, ${theme}] ${warning.trim()}`);
    return `<div class="cl-surface cl-${theme}">${content}</div>`;
  };
  return `${SURFACE_STYLE}${variant('light')}${variant('dark')}`;
}
