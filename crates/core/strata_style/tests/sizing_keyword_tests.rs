//! Tailwind sizing keyword and arbitrary-value mapping coverage.

use strata_style::prelude::*;

#[test]
fn test_tailwind_max_width_keywords_and_scale() {
    let cases = [
        ("max-w-none", Dimension::Auto),
        ("max-w-xs", Dimension::Px(320.0)),
        ("max-w-sm", Dimension::Px(384.0)),
        ("max-w-md", Dimension::Px(448.0)),
        ("max-w-lg", Dimension::Px(512.0)),
        ("max-w-xl", Dimension::Px(576.0)),
        ("max-w-2xl", Dimension::Px(672.0)),
        ("max-w-3xl", Dimension::Px(768.0)),
        ("max-w-4xl", Dimension::Px(896.0)),
        ("max-w-5xl", Dimension::Px(1024.0)),
        ("max-w-6xl", Dimension::Px(1152.0)),
        ("max-w-7xl", Dimension::Px(1280.0)),
        ("max-w-full", Dimension::Percent(100.0)),
        ("max-w-min", Dimension::MinContent),
        ("max-w-max", Dimension::MaxContent),
        ("max-w-fit", Dimension::FitContent(0.0)),
    ];

    for (class, expected) in cases {
        let style = TailwindEngine::resolve_classes(class);
        assert_eq!(style.max_width, expected, "class {class}");
    }
}

#[test]
fn test_tailwind_max_width_arbitrary() {
    let style = TailwindEngine::resolve_classes("max-w-[420px]");
    assert_eq!(style.max_width, Dimension::Px(420.0));

    let style = TailwindEngine::resolve_classes("max-w-[40rem]");
    assert_eq!(style.max_width, Dimension::Px(640.0));

    let style = TailwindEngine::resolve_classes("max-w-[75%]");
    assert_eq!(style.max_width, Dimension::Percent(75.0));
}

#[test]
fn test_tailwind_width_and_height_keywords() {
    assert_eq!(
        TailwindEngine::resolve_classes("w-max").width,
        Dimension::MaxContent
    );
    assert_eq!(
        TailwindEngine::resolve_classes("w-min").width,
        Dimension::MinContent
    );
    assert_eq!(
        TailwindEngine::resolve_classes("w-fit").width,
        Dimension::FitContent(0.0)
    );
    assert_eq!(
        TailwindEngine::resolve_classes("h-max").height,
        Dimension::MaxContent
    );
    assert_eq!(
        TailwindEngine::resolve_classes("w-[420px]").width,
        Dimension::Px(420.0)
    );
    assert_eq!(
        TailwindEngine::resolve_classes("h-[12rem]").height,
        Dimension::Px(192.0)
    );
}

#[test]
fn test_flex_one_with_max_width_resolves_both_constraints() {
    let style = TailwindEngine::resolve_classes("flex-1 max-w-md");
    assert_eq!(style.flex_grow, 1.0);
    assert_eq!(style.flex_shrink, 1.0);
    assert_eq!(style.flex_basis, Dimension::Percent(0.0));
    assert_eq!(style.max_width, Dimension::Px(448.0));
}

#[test]
fn test_width_keywords_cover_max_content_and_arbitrary_values() {
    assert_eq!(
        TailwindEngine::resolve_classes("w-max").width,
        Dimension::MaxContent
    );
    assert_eq!(
        TailwindEngine::resolve_classes("max-w-[360px]").max_width,
        Dimension::Px(360.0)
    );
    assert_eq!(
        TailwindEngine::resolve_classes("max-h-[240px]").max_height,
        Dimension::Px(240.0)
    );
}
