-- Neovim nvim-lspconfig configuration recipe for Strata Language Server (strata-lsp)

local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.strata_lsp then
  configs.strata_lsp = {
    default_config = {
      cmd = { 'strata-lsp' },
      filetypes = { 'rust' },
      root_dir = lspconfig.util.root_pattern('Cargo.toml', 'strata.yaml', '.git'),
      settings = {},
    },
  }
end

lspconfig.strata_lsp.setup {
  on_attach = function(client, bufnr)
    -- Enable document color provider if supported
    if client.server_capabilities.colorProvider then
      vim.lsp.buf.document_color()
    end
  end,
}
