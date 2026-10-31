import { defineConfig } from 'vitepress';

export default defineConfig({
  title: 'Strata',
  description: 'The High-Performance, Pure Rust Universal Frontend Engine',
  lang: 'en-US',
  cleanUrls: true,
  lastUpdated: true,

  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' }],
    ['meta', { name: 'theme-color', content: '#08090a' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:title', content: 'Strata | Zero-Chromium Pure Rust Universal Frontend Engine' }],
    ['meta', { property: 'og:description', content: 'Zero Chromium. Pixel-perfect CSS & Flexbox/Grid emulation. React/TSX-grade ergonomics. Native 120 FPS GPU rendering across Desktop, Mobile, Web, and Terminal.' }],
    ['meta', { property: 'og:url', content: 'https://strata.larvance.com' }],
    ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
    ['meta', { name: 'twitter:site', content: '@larvance' }]
  ],

  themeConfig: {
    logo: '/logo.svg',
    siteTitle: 'Strata',

    nav: [
      { text: 'Guide', link: '/guide/introduction' },
      { text: 'Architecture', link: '/architecture/overview' },
      { text: 'Ecosystem', link: '/ecosystem/developer-tools' },
      { text: 'Benchmarks', link: '/benchmarks/scorecard' },
      { text: 'Showcase', link: '/showcase/examples' },
      {
        text: 'v0.1.0',
        items: [
          { text: 'GitHub Repository', link: 'https://github.com/larvance/strata' },
          { text: 'Release Notes', link: 'https://github.com/larvance/strata/releases' }
        ]
      }
    ],

    sidebar: {
      '/guide/': [
        {
          text: 'Getting Started',
          items: [
            { text: 'Introduction', link: '/guide/introduction' },
            { text: 'Quick Start', link: '/guide/quick-start' },
            { text: 'CLI Toolchain', link: '/guide/cli' }
          ]
        },
        {
          text: 'Framework Concepts',
          items: [
            { text: 'Components & RSX', link: '/guide/components' },
            { text: 'Tailwind CSS JIT', link: '/guide/styling' },
            { text: 'Reactive Signals', link: '/guide/signals' }
          ]
        }
      ],
      '/architecture/': [
        {
          text: 'Core Architecture',
          items: [
            { text: 'System Topology', link: '/architecture/overview' },
            { text: 'DOM & UiIrNode', link: '/architecture/dom-and-ir' },
            { text: 'Layout Engine', link: '/architecture/layout-engine' },
            { text: 'GPU & SDF Rendering', link: '/architecture/rendering-and-gpu' },
            { text: 'Parallel Reconciliation', link: '/architecture/parallel-engine' },
            { text: 'Virtual FS & Storage', link: '/architecture/virtual-fs-storage' },
            { text: 'Terminal (TUI) Target', link: '/architecture/tui-target' },
            { text: 'Dual Web Compilation', link: '/architecture/dual-web' }
          ]
        }
      ],
      '/ecosystem/': [
        {
          text: 'Developer Ecosystem',
          items: [
            { text: 'DevTools & CDP Bridge', link: '/ecosystem/developer-tools' },
            { text: 'Language Server (LSP)', link: '/ecosystem/lsp-and-editor' },
            { text: 'Mobile & Simulator', link: '/ecosystem/mobile-and-simulator' },
            { text: 'Plugins & C-FFI', link: '/ecosystem/plugins-and-ffi' }
          ]
        }
      ],
      '/benchmarks/': [
        {
          text: 'Performance Verification',
          items: [
            { text: 'Technical Scorecard', link: '/benchmarks/scorecard' }
          ]
        }
      ],
      '/showcase/': [
        {
          text: 'Flagship Examples',
          items: [
            { text: 'Showcase Gallery', link: '/showcase/examples' }
          ]
        }
      ]
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/larvance/strata' }
    ],

    search: {
      provider: 'local'
    },

    footer: {
      message: 'Zero Chromium. Pure Rust. Native 120 FPS.',
      copyright: 'Copyright (c) 2026 larvance'
    },

    outline: {
      level: [2, 3],
      label: 'On this page'
    },

    docFooter: {
      prev: 'Previous Page',
      next: 'Next Page'
    }
  }
});
