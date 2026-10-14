import type { APIRoute, GetStaticPaths } from 'astro';
import { allIndicatorSpecs, chartFile, type Spec } from '../../chartlet';

// The indicator charts of the showcase as files, light and dark, so the showcase page can load
// them lazily instead of carrying 130 inline SVGs.
export const getStaticPaths = (() =>
  allIndicatorSpecs().flatMap(([name, spec]) =>
    (['light', 'dark'] as const).map((theme) => ({
      params: { chart: `${name}-${theme}` },
      props: { spec, name, theme },
    })),
  )) satisfies GetStaticPaths;

export const GET: APIRoute = ({ props }) => {
  const { spec, name, theme } = props as { spec: Spec; name: string; theme: 'light' | 'dark' };
  return new Response(chartFile(spec, name, theme), { headers: { 'Content-Type': 'image/svg+xml' } });
};
