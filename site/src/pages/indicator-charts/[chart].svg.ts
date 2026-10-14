import type { APIRoute, GetStaticPaths } from 'astro';
import { renderChart } from '@casoon/chartlet';

type Spec = Record<string, unknown>;

// One chartlet specification per indicator, written by `cargo run --example site_showcase` into
// site/.generated/indicators/ (not committed; the Pages workflow runs the example first).
const specs = import.meta.glob<Spec>('../../../.generated/indicators/*.json', {
  import: 'default',
  eager: true,
});

export const getStaticPaths = (() =>
  Object.entries(specs).flatMap(([path, spec]) => {
    const name = path.split('/').pop()!.replace('.json', '');
    return (['light', 'dark'] as const).map((theme) => ({
      params: { chart: `${name}-${theme}` },
      props: { spec, name, theme },
    }));
  })) satisfies GetStaticPaths;

export const GET: APIRoute = ({ props }) => {
  const { spec, name, theme } = props as { spec: Spec; name: string; theme: 'light' | 'dark' };
  const { content, warnings } = renderChart(
    { ...spec, theme },
    { format: 'svg', idPrefix: `${name}-${theme}` },
  );
  for (const warning of warnings) console.warn(`[chartlet ${name}, ${theme}] ${warning.trim()}`);
  return new Response(content, { headers: { 'Content-Type': 'image/svg+xml' } });
};
