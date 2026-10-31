# Language Server Protocol & Editors

Strata includes a full-featured **Language Server Protocol 3.17** implementation (`strata_lsp`) delivering intelligent autocompletion, real-time diagnostics, and hover documentation inside your favorite code editor.

---

## Language Server Capabilities

### 1. RSX Autocompletion & Tag Matching
- Autocompletes built-in HTML tags (`div`, `span`, `button`, `input`) and custom Strata components.
- Automatically inserts closing tags and verifies matching tag pairs.
- Suggests component props with exact Rust type annotations.

### 2. Tailwind CSS Autocomplete & Hover Docs
- Provides context-aware completions inside `class="..."` strings.
- Shows instant hover previews of computed CSS styles (e.g., hovering `p-4` displays `padding: 1rem /* 16px */`).
- Flags unrecognized or malformed Tailwind classes at edit time with instant diagnostics.

### 3. Syntax Diagnostics & Linting
- Catches unclosed RSX fragments (`<> ... </>`).
- Detects dangling reactive signal closures missing the `move` keyword.
- Warns when graphics-only components (`<Canvas3D>`) are used in projects targeting `--target tui`.

---

## Editor Integrations

### VS Code Extension

Install the official Strata extension from the Visual Studio Code Marketplace or package locally:

```bash
cd editors/vscode
npm install
npm run package
code --install-extension strata-0.1.0.vsix
```

Configuration in `.vscode/settings.json`:
```json
{
  "strata.lsp.serverPath": "strata-lsp",
  "strata.autocomplete.tailwind": true,
  "strata.hover.cssPreview": true
}
```

---

### Zed Editor

Configure Zed by adding Strata to your `~/.config/zed/settings.json`:

```json
{
  "lsp": {
    "strata-lsp": {
      "binary": {
        "path": "strata-lsp"
      }
    }
  },
  "languages": {
    "Rust": {
      "language_servers": ["rust-analyzer", "strata-lsp"]
    }
  }
}
```

---

### Neovim (`nvim-lspconfig`)

Add the following to your Neovim Lua configuration (`init.lua`):

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.strata_lsp then
  configs.strata_lsp = {
    default_config = {
      cmd = { 'strata-lsp' },
      filetypes = { 'rust' },
      root_dir = lspconfig.util.root_pattern('Cargo.toml', 'strata.toml'),
      settings = {},
    },
  }
end

lspconfig.strata_lsp.setup{}
```

---

### Helix Editor

Configure `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "rust"
language-servers = ["rust-analyzer", "strata-lsp"]

[language-server.strata-lsp]
command = "strata-lsp"
```
