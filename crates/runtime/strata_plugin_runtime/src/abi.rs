//! Application Binary Interface (ABI) definitions for dynamic plugins.

pub const STRATA_PLUGIN_ABI_VERSION: u32 = 1;
pub const STRATA_PLUGIN_MAGIC: [u8; 4] = *b"STPL";

/// Binary header prepended to compiled Strata dynamic plugin bundles.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginBinaryHeader {
    pub magic: [u8; 4],
    pub abi_version: u32,
    pub manifest_offset: u32,
    pub manifest_length: u32,
}

impl PluginBinaryHeader {
    pub fn new(manifest_offset: u32, manifest_length: u32) -> Self {
        Self {
            magic: STRATA_PLUGIN_MAGIC,
            abi_version: STRATA_PLUGIN_ABI_VERSION,
            manifest_offset,
            manifest_length,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == STRATA_PLUGIN_MAGIC && self.abi_version == STRATA_PLUGIN_ABI_VERSION
    }
}
