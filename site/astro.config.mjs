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
        'Rust technical analysis library: streaming indicators, market regime classification, composite scoring and SVG charts.',
      repo: 'casoon/kestrel-chartkit',
      version: '0.11.3',
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
      // No CHANGELOG.md and no release notes yet.
      changelog: false,
    }),
  ],
});
