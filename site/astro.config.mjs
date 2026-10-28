// @ts-check
import casoonPages from '@casoon/pages-theme';
import { defineConfig } from 'astro/config';

// Project page: https://casoon.github.io/kestrel-chartkit/ — `base` is the GitHub Pages path.
export default defineConfig({
  site: 'https://casoon.github.io/kestrel-chartkit',
  base: '/kestrel-chartkit/',
  integrations: [
    casoonPages({
      name: 'kestrel-chartkit',
      description:
        'Rust technical analysis library, in development: streaming indicators, market regime classification, composite scoring and SVG charts.',
      tagline: 'Technical analysis library for Rust',
      repo: 'casoon/kestrel-chartkit',
      license: 'BUSL-1.1',
      branch: 'master',
      packages: [
        { label: 'crates.io', href: 'https://crates.io/crates/kestrel-chartkit' },
        { label: 'docs.rs', href: 'https://docs.rs/kestrel-chartkit' },
      ],
      docsGroups: {
        'getting-started': 'Getting started',
        guides: 'Guides',
        reference: 'Reference',
      },
      mdxComponents: './src/mdx-components.ts',
      showcaseLead:
        'Every indicator of the catalog, drawn from what kestrel-chartkit computes: default parameters on synthetic 4-hour bars of a fictitious CASOON share with invented prices, the latest alerts as markers. chartlet does the drawing. Pick an indicator for its chart and the Rust call that builds it.',
    }),
  ],
});
