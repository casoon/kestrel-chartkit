import { renderChart } from '@casoon/chartlet';

export type Spec = Record<string, unknown> & { description?: string; width?: number; height?: number };

export interface IndicatorEntry {
  name: string;
  family: string;
  description: string;
  unit: string;
  chart: boolean;
  defaults: Record<string, number>;
}

/**
 * chartlet specifications written by `cargo run --example site_showcase` into site/.generated/
 * (not committed; the Pages workflow runs the example before the site build). Every number in
 * them is computed by kestrel-chartkit, chartlet only draws.
 */
const specs = import.meta.glob<Spec>('../.generated/*.json', { import: 'default', eager: true });
const indicatorSpecs = import.meta.glob<Spec>('../.generated/indicators/*.json', {
  import: 'default',
  eager: true,
});

function required<T>(value: T | undefined, what: string): T {
  if (value === undefined) {
    throw new Error(`Missing site/.generated/${what}; run: cargo run --example site_showcase`);
  }
  return value;
}

/** The indicator list in family order, as the example wrote it. */
export const indicators = required(
  specs['../.generated/indicators.json'] as unknown as IndicatorEntry[] | undefined,
  'indicators.json',
);

export function spec(name: string): Spec {
  return required(specs[`../.generated/${name}.json`], `${name}.json`);
}

export function indicatorSpec(name: string): Spec {
  return required(indicatorSpecs[`../.generated/indicators/${name}.json`], `indicators/${name}.json`);
}

export function allIndicatorSpecs(): [string, Spec][] {
  return Object.entries(indicatorSpecs).map(([path, s]) => [path.split('/').pop()!.replace('.json', ''), s]);
}

/**
 * The variables of chartlet's dark theme for one chart, read from chartlet's own dark output, so
 * one SVG serves both themes: a light chart on a light page, the dark palette on a dark page (the
 * approach of the chartlet site). Per chart, because each family — time series, diagrams —
 * declares its own variables.
 */
function darkVariables(source: Spec): string {
  const variables = new Map<string, string>();
  const { content } = renderChart({ ...source, theme: 'dark' }, { format: 'svg' });
  for (const rule of content.matchAll(/\.chartlet-theme-dark\{([^}]*)\}/g)) {
    for (const declaration of rule[1].split(';')) {
      const colon = declaration.indexOf(':');
      const name = declaration.slice(0, colon).trim();
      if (name.startsWith('--chartlet-')) variables.set(name, declaration.slice(colon + 1).trim());
    }
  }
  if (!variables.has('--chartlet-text')) throw new Error("chartlet's dark theme no longer declares --chartlet-text");
  return [...variables].map(([name, value]) => `${name}:${value}`).join(';');
}

/** A chart inline: one SVG that follows the page's theme. */
export function inlineChart(source: Spec, id: string): string {
  const { content, warnings } = renderChart({ ...source, theme: 'light' }, { format: 'svg', idPrefix: id });
  for (const warning of warnings) console.warn(`[chartlet ${id}] ${warning.trim()}`);
  const dark = darkVariables(source);
  const scope = `.cl-${id}`;
  const style = `<style>.cl-surface{background:#fff;color:#172033;color-scheme:light;padding:12px;border-radius:10px;line-height:0;overflow-x:auto}.cl-surface svg{max-width:100%;height:auto}:root[data-theme=dark] .cl-surface{background:#0e131c;color:#e8edf6;color-scheme:dark}:root[data-theme=dark] ${scope} .chartlet-root{${dark}}@media (prefers-color-scheme:dark){:root:not([data-theme=light]) .cl-surface{background:#0e131c;color:#e8edf6;color-scheme:dark}:root:not([data-theme=light]) ${scope} .chartlet-root{${dark}}}</style>`;
  return `${style}<div class="cl-surface cl-${id}">${content}</div>`;
}

/** A chart as a standalone SVG file (no page CSS reaches it), so one per theme. */
export function chartFile(source: Spec, id: string, theme: 'light' | 'dark'): string {
  const { content, warnings } = renderChart({ ...source, theme }, { format: 'svg', idPrefix: `${id}-${theme}` });
  for (const warning of warnings) console.warn(`[chartlet ${id}, ${theme}] ${warning.trim()}`);
  return content;
}
