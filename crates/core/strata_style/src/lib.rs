//! Strata Style: CSS spec parser, specificity calculator, cascade solver, and Tailwind JIT compiler.

pub mod animation;
pub mod bezier;
pub mod calc;
pub mod cascade;
pub mod effects;
pub mod keyframes;
pub mod matrix_decomp;
pub mod media;
pub mod oklab;
pub mod palette;
pub mod parser;
pub mod properties;
pub mod selector;
pub mod specificity;
pub mod tailwind;
pub mod transition;
pub mod types;
pub mod ua_stylesheet;

pub use animation::{
    ActiveKeyframeAnimation, AnimationLifecycleEvent, AnimationManager, CompiledKeyframeStop,
    KeyframeRegistry, KeyframeTrack,
};
pub use bezier::cubic_bezier_solve;
pub use calc::{get_calc, intern_calc, parse_calc, CalcContext, CalcExpr, CalcId};
pub use cascade::{CascadeOrigin, CascadeSolver, OriginStyleSheet};
pub use effects::{interpolate_clip_path, BackdropFilter, ClipPath, FilterList, FilterOp};
pub use matrix_decomp::{slerp_quaternion, Decomposed2d};
pub use oklab::{interpolate_color_oklab, oklab_to_rgba, rgba_to_oklab};
pub use keyframes::{CubicBezier, Keyframe, KeyframeAnimation, KeyframeProperty};
pub use media::{
    ColorScheme, DevicePostureQuery, MediaContext, MediaFeature, MediaQuery, MediaQueryList,
    MediaType, Orientation,
};
pub use palette::lookup_tailwind_color;
pub use parser::{
    parse_animation_delays, parse_animation_directions, parse_animation_durations,
    parse_animation_fill_modes, parse_animation_iteration_counts, parse_animation_names,
    parse_animation_play_states, parse_animation_range, parse_animation_timeline,
    parse_animation_timing_functions, parse_animations, parse_clip_path, parse_color,
    parse_declarations, parse_dimension, parse_edge_insets_px, parse_filter_list,
    parse_grid_tracks, parse_keyframe_rule_body, parse_keyframe_stop_offset,
    parse_single_animation, parse_single_transition, parse_time_ms, parse_transition_behaviors,
    parse_transition_delays, parse_transition_durations, parse_transition_properties,
    parse_transition_timing_functions, parse_transitions, parse_view_transition_name, Declaration,
    KeyframeRule, KeyframeStopRule, StartingStyleRule, StyleRule, StyleSheet,
};
pub use properties::{
    parse_property_rule_body, PropertyRegistry, PropertySyntax, RegisteredProperty,
};
pub use selector::{
    AttributeMatchOp, AttributeSelector, Combinator, CompoundSelector, ElementRef, PseudoState,
    Selector, SelectorPart,
};
pub use specificity::Specificity;
pub use tailwind::TailwindEngine;
pub use transition::{
    apply_animatable_value, extract_animatable_value, find_transition_spec,
    interpolate_border_radius, interpolate_box_shadow, interpolate_color, interpolate_dimension,
    interpolate_edge_insets, ActiveTransition, AnimatableValue, TransitionLifecycleEvent,
    TransitionManager,
};
pub use types::*;
pub use ua_stylesheet::{get_ua_stylesheet, W3C_UA_STYLESHEET_CSS};

pub mod prelude {
    pub use crate::animation::*;
    pub use crate::calc::{get_calc, intern_calc, parse_calc, CalcContext, CalcExpr, CalcId};
    pub use crate::cascade::{CascadeOrigin, CascadeSolver, OriginStyleSheet};
    pub use crate::effects::{BackdropFilter, ClipPath, FilterOp};
    pub use crate::keyframes::{CubicBezier, Keyframe, KeyframeAnimation, KeyframeProperty};
    pub use crate::media::{
        ColorScheme, MediaContext, MediaFeature, MediaQuery, MediaQueryList, MediaType, Orientation,
    };
    pub use crate::palette::lookup_tailwind_color;
    pub use crate::parser::{Declaration, KeyframeRule, KeyframeStopRule, StyleRule, StyleSheet};
    pub use crate::selector::{
        AttributeMatchOp, AttributeSelector, Combinator, CompoundSelector, ElementRef, PseudoState,
        Selector, SelectorPart,
    };
    pub use crate::specificity::Specificity;
    pub use crate::tailwind::TailwindEngine;
    pub use crate::transition::*;
    pub use crate::types::*;
    pub use crate::ua_stylesheet::{get_ua_stylesheet, W3C_UA_STYLESHEET_CSS};
}
