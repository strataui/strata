use strata_render::font::{
    get_glyph, get_glyph_or_fallback, FontFallbackChain, GenericFontFamily, OsPlatform,
    SystemFontResolver, TOFU_GLYPH,
};

#[test]
fn test_system_font_resolver_platforms() {
    let mac_chain = SystemFontResolver::chain_for_platform(GenericFontFamily::SansSerif, OsPlatform::MacOS);
    assert!(mac_chain.contains(&"-apple-system".to_string()));
    assert!(mac_chain.contains(&"SF Pro Text".to_string()));

    let win_chain = SystemFontResolver::chain_for_platform(GenericFontFamily::SansSerif, OsPlatform::Windows);
    assert!(win_chain.contains(&"Segoe UI".to_string()));
    assert!(win_chain.contains(&"Microsoft YaHei".to_string()));

    let linux_chain = SystemFontResolver::chain_for_platform(GenericFontFamily::SansSerif, OsPlatform::Linux);
    assert!(linux_chain.contains(&"Ubuntu".to_string()) || linux_chain.contains(&"Noto Sans".to_string()));
    assert!(linux_chain.contains(&"sans-serif".to_string()));
}

#[test]
fn test_font_family_css_parsing_cascade() {
    let cascade = SystemFontResolver::parse_font_family_cascade("-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif");
    assert!(cascade.len() >= 4);
    assert_eq!(cascade[0], "-apple-system");
    assert!(cascade.contains(&"Segoe UI".to_string()));
    assert!(cascade.contains(&"Roboto".to_string()));
}

#[test]
fn test_glyph_resolution_and_fallback_tofu() {
    // Printable ASCII
    let ascii_a = get_glyph('A');
    assert!(ascii_a.is_some());

    // Common Unicode typography mappings
    let curly_quote = get_glyph_or_fallback('“');
    let straight_quote = get_glyph('"').unwrap();
    assert_eq!(curly_quote, straight_quote);

    let em_dash = get_glyph_or_fallback('—');
    let hyphen = get_glyph('-').unwrap();
    assert_eq!(em_dash, hyphen);

    // Unresolved character falls back to TOFU
    let cjk = get_glyph_or_fallback('字');
    assert_eq!(cjk, &TOFU_GLYPH);

    let emoji = get_glyph_or_fallback('🦀');
    assert_eq!(emoji, &TOFU_GLYPH);
}

#[test]
fn test_font_fallback_chain_can_render() {
    let chain = FontFallbackChain::from_css("Inter, sans-serif");
    assert_eq!(chain.primary_font(), Some("Inter"));
    assert!(chain.can_render('H'));
    assert!(chain.can_render('’'));
    assert!(!chain.can_render('漢'));
}
