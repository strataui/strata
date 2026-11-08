//! High-performance CSS keyframe animation runtime, track sampling, and iteration engine.

use std::collections::HashMap;

use crate::parser::{
    parse_angle_rad, parse_color, parse_dimension, parse_edge_insets_px, parse_transform_list,
    Declaration, KeyframeRule, StyleSheet,
};
use crate::transition::{
    apply_animatable_value, extract_animatable_value, interpolate_border_radius,
    interpolate_box_shadow, interpolate_color, interpolate_dimension, interpolate_edge_insets,
    interpolate_transform_ops, AnimatableValue,
};
use crate::types::{
    AnimationDirection, AnimationFillMode, AnimationIterationCount, AnimationPlayState,
    AnimationSpec, AnimationTimeline, ComputedStyle, Dimension, TransformOp, TransitionEasing,
    TransitionProperty,
};

/// A compiled stop in a keyframe animation timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledKeyframeStop {
    /// Normalized timeline offset in [0.0, 1.0].
    pub offset: f32,
    /// Property values specified at this keyframe.
    pub properties: HashMap<TransitionProperty, AnimatableValue>,
    /// Optional segment easing starting from this keyframe.
    pub timing_function: Option<TransitionEasing>,
}

/// A compiled timeline track representing a named CSS `@keyframes` animation.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeTrack {
    pub name: String,
    pub stops: Vec<CompiledKeyframeStop>,
}

impl KeyframeTrack {
    /// Constructs a compiled `KeyframeTrack` from a parsed `KeyframeRule`.
    pub fn from_rule(rule: &KeyframeRule) -> Self {
        let mut stops = Vec::with_capacity(rule.stops.len());

        for stop_rule in &rule.stops {
            let mut props = HashMap::new();
            for decl in &stop_rule.declarations {
                if let Some((prop, val)) = declaration_to_animatable(decl) {
                    props.insert(prop, val);
                }
            }
            stops.push(CompiledKeyframeStop {
                offset: stop_rule.offset,
                properties: props,
                timing_function: stop_rule.timing_function,
            });
        }

        stops.sort_by(|a, b| {
            a.offset
                .partial_cmp(&b.offset)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Self {
            name: rule.name.clone(),
            stops,
        }
    }

    /// Evaluates and samples animated property values at progress fraction `progress` in [0.0, 1.0].
    pub fn sample(
        &self,
        progress: f32,
        base_style: &ComputedStyle,
    ) -> HashMap<TransitionProperty, AnimatableValue> {
        let mut result = HashMap::new();
        if self.stops.is_empty() {
            return result;
        }

        let p = progress.clamp(0.0, 1.0);
        let first = &self.stops[0];
        let last = &self.stops[self.stops.len() - 1];

        if p <= first.offset {
            return first.properties.clone();
        }
        if p >= last.offset {
            return last.properties.clone();
        }

        let mut k1 = first;
        let mut k2 = last;

        for i in 0..self.stops.len() - 1 {
            if self.stops[i].offset <= p && p <= self.stops[i + 1].offset {
                k1 = &self.stops[i];
                k2 = &self.stops[i + 1];
                break;
            }
        }

        let span = k2.offset - k1.offset;
        let mut alpha = if span > 0.0 {
            (p - k1.offset) / span
        } else {
            0.0
        };

        // Apply segment timing function if specified on the starting keyframe
        if let Some(easing) = k1.timing_function {
            alpha = easing.solve(alpha);
        }

        // Collect all distinct property keys present across k1, k2, or the base style
        let mut all_keys = std::collections::HashSet::new();
        for k in k1.properties.keys() {
            all_keys.insert(k.clone());
        }
        for k in k2.properties.keys() {
            all_keys.insert(k.clone());
        }

        for key in all_keys {
            let v1 = k1
                .properties
                .get(&key)
                .cloned()
                .or_else(|| extract_animatable_value(base_style, &key));
            let v2 = k2
                .properties
                .get(&key)
                .cloned()
                .or_else(|| extract_animatable_value(base_style, &key));

            if let (Some(val1), Some(val2)) = (v1, v2) {
                let interpolated = interpolate_animatable_value(&val1, &val2, alpha);
                result.insert(key, interpolated);
            }
        }

        result
    }
}

/// Converts a single CSS `Declaration` into an animatable property and value pair.
fn declaration_to_animatable(decl: &Declaration) -> Option<(TransitionProperty, AnimatableValue)> {
    let prop_name = decl.property.as_str();
    let val_str = decl.value.as_str();

    match prop_name {
        "opacity" => {
            let n = val_str.parse::<f32>().ok()?;
            Some((TransitionProperty::Opacity, AnimatableValue::Number(n)))
        }
        "transform" => {
            let ops = parse_transform_list(val_str);
            Some((
                TransitionProperty::Transform,
                AnimatableValue::Transform(ops),
            ))
        }
        "rotate" => {
            let rad = parse_angle_rad(val_str)?;
            Some((
                TransitionProperty::Transform,
                AnimatableValue::Transform(vec![TransformOp::Rotate(rad)]),
            ))
        }
        "scale" => {
            let parts: Vec<&str> = val_str.split_whitespace().collect();
            let first = parts.first().and_then(|s| s.parse::<f32>().ok())?;
            let second = parts
                .get(1)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(first);
            Some((
                TransitionProperty::Transform,
                AnimatableValue::Transform(vec![TransformOp::Scale(first, second)]),
            ))
        }
        "translate" => {
            let parts: Vec<&str> = val_str.split_whitespace().collect();
            let dx = parts.first().and_then(|s| parse_dimension(s))?;
            let dy = parts
                .get(1)
                .and_then(|s| parse_dimension(s))
                .unwrap_or(Dimension::Px(0.0));
            Some((
                TransitionProperty::Transform,
                AnimatableValue::Transform(vec![TransformOp::Translate(dx, dy)]),
            ))
        }
        "width" => {
            let d = parse_dimension(val_str)?;
            Some((TransitionProperty::Width, AnimatableValue::Dimension(d)))
        }
        "height" => {
            let d = parse_dimension(val_str)?;
            Some((TransitionProperty::Height, AnimatableValue::Dimension(d)))
        }
        "color" => {
            let c = parse_color(val_str)?;
            Some((TransitionProperty::Color, AnimatableValue::Color(c)))
        }
        "background-color" | "background" => {
            let c = parse_color(val_str)?;
            Some((
                TransitionProperty::BackgroundColor,
                AnimatableValue::Color(c),
            ))
        }
        "border-color" => {
            let c = parse_color(val_str)?;
            Some((TransitionProperty::BorderColor, AnimatableValue::Color(c)))
        }
        "border-radius" => {
            let insets = parse_edge_insets_px(val_str)?;
            let br = crate::types::BorderRadius::new(
                insets.top,
                insets.right,
                insets.bottom,
                insets.left,
            );
            Some((
                TransitionProperty::BorderRadius,
                AnimatableValue::BorderRadius(br),
            ))
        }
        "margin" => {
            let m = parse_edge_insets_px(val_str)?;
            Some((TransitionProperty::Margin, AnimatableValue::EdgeInsets(m)))
        }
        "padding" => {
            let p = parse_edge_insets_px(val_str)?;
            Some((TransitionProperty::Padding, AnimatableValue::EdgeInsets(p)))
        }
        "clip-path" => {
            let cp = crate::parser::parse_clip_path(val_str)?;
            Some((TransitionProperty::ClipPath, AnimatableValue::ClipPath(cp)))
        }
        _ => None,
    }
}

/// Interpolates two `AnimatableValue` variants given normalized progress in [0.0, 1.0].
pub fn interpolate_animatable_value(
    v1: &AnimatableValue,
    v2: &AnimatableValue,
    progress: f32,
) -> AnimatableValue {
    match (v1, v2) {
        (AnimatableValue::Number(a), AnimatableValue::Number(b)) => {
            AnimatableValue::Number(a + (b - a) * progress)
        }
        (AnimatableValue::Color(c1), AnimatableValue::Color(c2)) => {
            AnimatableValue::Color(interpolate_color(*c1, *c2, progress))
        }
        (AnimatableValue::Dimension(d1), AnimatableValue::Dimension(d2)) => {
            AnimatableValue::Dimension(interpolate_dimension(d1, d2, progress))
        }
        (AnimatableValue::EdgeInsets(e1), AnimatableValue::EdgeInsets(e2)) => {
            AnimatableValue::EdgeInsets(interpolate_edge_insets(e1, e2, progress))
        }
        (AnimatableValue::BorderRadius(r1), AnimatableValue::BorderRadius(r2)) => {
            AnimatableValue::BorderRadius(interpolate_border_radius(r1, r2, progress))
        }
        (AnimatableValue::BoxShadow(s1), AnimatableValue::BoxShadow(s2)) => {
            AnimatableValue::BoxShadow(interpolate_box_shadow(s1, s2, progress))
        }
        (AnimatableValue::Transform(t1), AnimatableValue::Transform(t2)) => {
            AnimatableValue::Transform(interpolate_transform_ops(t1, t2, progress))
        }
        (AnimatableValue::ClipPath(c1), AnimatableValue::ClipPath(c2)) => {
            AnimatableValue::ClipPath(crate::effects::interpolate_clip_path(c1, c2, progress))
        }
        _ => v2.clone(),
    }
}

/// Registry of available named `@keyframes` tracks (including default Tailwind animations).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeRegistry {
    tracks: HashMap<String, KeyframeTrack>,
}

impl Default for KeyframeRegistry {
    fn default() -> Self {
        let mut reg = Self {
            tracks: HashMap::new(),
        };
        reg.register_builtin_tailwind_tracks();
        reg
    }
}

impl KeyframeRegistry {
    /// Creates a new `KeyframeRegistry` populated with standard Tailwind built-in animation tracks.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the default Tailwind CSS keyframe animation tracks (`spin`, `ping`, `pulse`, `bounce`).
    fn register_builtin_tailwind_tracks(&mut self) {
        // 1. spin: from { transform: rotate(0deg); } to { transform: rotate(360deg); }
        let mut spin_0 = HashMap::new();
        spin_0.insert(
            TransitionProperty::Transform,
            AnimatableValue::Transform(vec![TransformOp::Rotate(0.0)]),
        );
        let mut spin_1 = HashMap::new();
        spin_1.insert(
            TransitionProperty::Transform,
            AnimatableValue::Transform(vec![TransformOp::Rotate(2.0 * std::f32::consts::PI)]),
        );
        self.tracks.insert(
            "spin".to_string(),
            KeyframeTrack {
                name: "spin".to_string(),
                stops: vec![
                    CompiledKeyframeStop {
                        offset: 0.0,
                        properties: spin_0,
                        timing_function: None,
                    },
                    CompiledKeyframeStop {
                        offset: 1.0,
                        properties: spin_1,
                        timing_function: None,
                    },
                ],
            },
        );

        // 2. ping: 75%, 100% { transform: scale(2); opacity: 0; } (0% defaults to scale(1), opacity: 1)
        let mut ping_0 = HashMap::new();
        ping_0.insert(
            TransitionProperty::Transform,
            AnimatableValue::Transform(vec![TransformOp::Scale(1.0, 1.0)]),
        );
        ping_0.insert(TransitionProperty::Opacity, AnimatableValue::Number(1.0));

        let mut ping_75 = HashMap::new();
        ping_75.insert(
            TransitionProperty::Transform,
            AnimatableValue::Transform(vec![TransformOp::Scale(2.0, 2.0)]),
        );
        ping_75.insert(TransitionProperty::Opacity, AnimatableValue::Number(0.0));

        self.tracks.insert(
            "ping".to_string(),
            KeyframeTrack {
                name: "ping".to_string(),
                stops: vec![
                    CompiledKeyframeStop {
                        offset: 0.0,
                        properties: ping_0,
                        timing_function: None,
                    },
                    CompiledKeyframeStop {
                        offset: 0.75,
                        properties: ping_75.clone(),
                        timing_function: None,
                    },
                    CompiledKeyframeStop {
                        offset: 1.0,
                        properties: ping_75,
                        timing_function: None,
                    },
                ],
            },
        );

        // 3. pulse: 50% { opacity: .5; }
        let mut pulse_0 = HashMap::new();
        pulse_0.insert(TransitionProperty::Opacity, AnimatableValue::Number(1.0));
        let mut pulse_50 = HashMap::new();
        pulse_50.insert(TransitionProperty::Opacity, AnimatableValue::Number(0.5));
        let mut pulse_100 = HashMap::new();
        pulse_100.insert(TransitionProperty::Opacity, AnimatableValue::Number(1.0));

        self.tracks.insert(
            "pulse".to_string(),
            KeyframeTrack {
                name: "pulse".to_string(),
                stops: vec![
                    CompiledKeyframeStop {
                        offset: 0.0,
                        properties: pulse_0,
                        timing_function: None,
                    },
                    CompiledKeyframeStop {
                        offset: 0.5,
                        properties: pulse_50,
                        timing_function: None,
                    },
                    CompiledKeyframeStop {
                        offset: 1.0,
                        properties: pulse_100,
                        timing_function: None,
                    },
                ],
            },
        );

        // 4. bounce: 0%, 100% { transform: translateY(-25%); } 50% { transform: none; }
        let mut bounce_up = HashMap::new();
        bounce_up.insert(
            TransitionProperty::Transform,
            AnimatableValue::Transform(vec![TransformOp::Translate(
                Dimension::Px(0.0),
                Dimension::Percent(-25.0),
            )]),
        );
        let mut bounce_down = HashMap::new();
        bounce_down.insert(
            TransitionProperty::Transform,
            AnimatableValue::Transform(vec![TransformOp::Translate(
                Dimension::Px(0.0),
                Dimension::Px(0.0),
            )]),
        );

        self.tracks.insert(
            "bounce".to_string(),
            KeyframeTrack {
                name: "bounce".to_string(),
                stops: vec![
                    CompiledKeyframeStop {
                        offset: 0.0,
                        properties: bounce_up.clone(),
                        timing_function: Some(TransitionEasing::CubicBezier(0.8, 0.0, 1.0, 1.0)),
                    },
                    CompiledKeyframeStop {
                        offset: 0.5,
                        properties: bounce_down,
                        timing_function: Some(TransitionEasing::CubicBezier(0.0, 0.0, 0.2, 1.0)),
                    },
                    CompiledKeyframeStop {
                        offset: 1.0,
                        properties: bounce_up,
                        timing_function: None,
                    },
                ],
            },
        );
    }

    /// Registers all parsed `@keyframes` rules from a stylesheet into this registry.
    pub fn register_stylesheet(&mut self, sheet: &StyleSheet) {
        for rule in &sheet.keyframes {
            self.register_rule(rule);
        }
    }

    /// Registers a single `KeyframeRule` into the registry.
    pub fn register_rule(&mut self, rule: &KeyframeRule) {
        let track = KeyframeTrack::from_rule(rule);
        self.tracks.insert(rule.name.clone(), track);
    }

    /// Registers an already compiled `KeyframeTrack`.
    pub fn register_track(&mut self, track: KeyframeTrack) {
        self.tracks.insert(track.name.clone(), track);
    }

    /// Looks up a registered track by animation name.
    pub fn get(&self, name: &str) -> Option<&KeyframeTrack> {
        self.tracks.get(name)
    }
}

/// Lifecycle events emitted during CSS keyframe animation execution (W3C CSS Animations Level 1).
#[derive(Debug, Clone, PartialEq)]
pub enum AnimationLifecycleEvent {
    Start {
        node_id: u64,
        name: String,
        elapsed_time_sec: f32,
    },
    Iteration {
        node_id: u64,
        name: String,
        elapsed_time_sec: f32,
    },
    End {
        node_id: u64,
        name: String,
        elapsed_time_sec: f32,
    },
    Cancel {
        node_id: u64,
        name: String,
        elapsed_time_sec: f32,
    },
}

/// An active running or paused instance of a named keyframe animation on a DOM node.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveKeyframeAnimation {
    pub name: String,
    pub spec: AnimationSpec,
    pub track: KeyframeTrack,
    pub start_time_ms: f64,
    pub current_iteration: u32,
    pub start_emitted: bool,
    pub end_emitted: bool,
    pub is_finished: bool,
    pub current_values: HashMap<TransitionProperty, AnimatableValue>,
}

impl ActiveKeyframeAnimation {
    /// Creates a new active keyframe animation starting at `now_ms`.
    pub fn new(spec: AnimationSpec, track: KeyframeTrack, now_ms: f64) -> Self {
        let name = spec.name.clone();
        Self {
            name,
            spec,
            track,
            start_time_ms: now_ms,
            current_iteration: 0,
            start_emitted: false,
            end_emitted: false,
            is_finished: false,
            current_values: HashMap::new(),
        }
    }

    /// Advances the animation to timestamp `now_ms` and computes updated animatable values.
    pub fn step(
        &mut self,
        node_id: u64,
        now_ms: f64,
        base_style: &ComputedStyle,
    ) -> Vec<AnimationLifecycleEvent> {
        let mut events = Vec::new();
        if self.is_finished {
            return events;
        }

        // If paused, maintain existing current values without advancing time
        if self.spec.play_state == AnimationPlayState::Paused {
            return events;
        }

        let elapsed_ms = (now_ms - self.start_time_ms).max(0.0) as f32;

        // 1. Delay phase
        if elapsed_ms < self.spec.delay_ms {
            if self.spec.fill_mode == AnimationFillMode::Backwards
                || self.spec.fill_mode == AnimationFillMode::Both
            {
                let start_progress = match self.spec.direction {
                    AnimationDirection::Reverse | AnimationDirection::AlternateReverse => 1.0,
                    _ => 0.0,
                };
                self.current_values = self.track.sample(start_progress, base_style);
            }
            return events;
        }

        let active_ms = elapsed_ms - self.spec.delay_ms;

        // 2. Start event
        if !self.start_emitted {
            self.start_emitted = true;
            events.push(AnimationLifecycleEvent::Start {
                node_id,
                name: self.name.clone(),
                elapsed_time_sec: 0.0,
            });
        }

        let dur = self.spec.duration_ms.max(1.0);
        let raw_iter = active_ms / dur;
        let iter_idx = raw_iter.floor() as u32;

        // 3. Completion check for finite iteration counts
        if let AnimationIterationCount::Count(max_count) = self.spec.iteration_count {
            if active_ms >= max_count * dur {
                self.is_finished = true;
                if !self.end_emitted {
                    self.end_emitted = true;
                    events.push(AnimationLifecycleEvent::End {
                        node_id,
                        name: self.name.clone(),
                        elapsed_time_sec: max_count * dur / 1000.0,
                    });
                }

                if self.spec.fill_mode == AnimationFillMode::Forwards
                    || self.spec.fill_mode == AnimationFillMode::Both
                {
                    // Compute final position taking direction into account
                    let final_iter_idx = (max_count - 0.0001).floor() as u32;
                    let final_frac = if max_count.fract() == 0.0 {
                        1.0
                    } else {
                        max_count.fract()
                    };
                    let solved_t = self.spec.timing_function.solve(final_frac);
                    let final_progress = match self.spec.direction {
                        AnimationDirection::Normal => solved_t,
                        AnimationDirection::Reverse => 1.0 - solved_t,
                        AnimationDirection::Alternate => {
                            if final_iter_idx.is_multiple_of(2) {
                                solved_t
                            } else {
                                1.0 - solved_t
                            }
                        }
                        AnimationDirection::AlternateReverse => {
                            if final_iter_idx.is_multiple_of(2) {
                                1.0 - solved_t
                            } else {
                                solved_t
                            }
                        }
                    };
                    self.current_values = self.track.sample(final_progress, base_style);
                } else {
                    self.current_values.clear();
                }

                return events;
            }
        }

        // 4. Iteration boundary event
        if iter_idx > self.current_iteration {
            self.current_iteration = iter_idx;
            events.push(AnimationLifecycleEvent::Iteration {
                node_id,
                name: self.name.clone(),
                elapsed_time_sec: (iter_idx as f32 * dur) / 1000.0,
            });
        }

        // 5. Active fractional progress calculation
        let frac = (active_ms % dur) / dur;
        let solved_t = self.spec.timing_function.solve(frac);

        let progress = match self.spec.direction {
            AnimationDirection::Normal => solved_t,
            AnimationDirection::Reverse => 1.0 - solved_t,
            AnimationDirection::Alternate => {
                if iter_idx.is_multiple_of(2) {
                    solved_t
                } else {
                    1.0 - solved_t
                }
            }
            AnimationDirection::AlternateReverse => {
                if iter_idx.is_multiple_of(2) {
                    1.0 - solved_t
                } else {
                    solved_t
                }
            }
        };

        self.current_values = self.track.sample(progress, base_style);

        events
    }

    /// Advances the animation according to a normalized timeline progress fraction in [0.0, 1.0] (for scroll-driven animations).
    pub fn step_progress(
        &mut self,
        node_id: u64,
        progress: f32,
        base_style: &ComputedStyle,
    ) -> Vec<AnimationLifecycleEvent> {
        let mut events = Vec::new();
        if self.spec.play_state == AnimationPlayState::Paused {
            return events;
        }

        if !self.start_emitted {
            self.start_emitted = true;
            events.push(AnimationLifecycleEvent::Start {
                node_id,
                name: self.name.clone(),
                elapsed_time_sec: 0.0,
            });
        }

        // Apply range boundaries if configured
        let range = &self.spec.range;
        let p_clamped = progress.clamp(0.0, 1.0);
        let range_span = (range.end_offset - range.start_offset).abs();
        let effective_p = if range_span > 0.0 {
            ((p_clamped - range.start_offset) / range_span).clamp(0.0, 1.0)
        } else {
            p_clamped
        };

        let solved_t = self.spec.timing_function.solve(effective_p);
        let final_progress = match self.spec.direction {
            AnimationDirection::Normal => solved_t,
            AnimationDirection::Reverse => 1.0 - solved_t,
            AnimationDirection::Alternate | AnimationDirection::AlternateReverse => solved_t,
        };

        self.current_values = self.track.sample(final_progress, base_style);

        if progress >= 1.0 && !self.end_emitted {
            self.end_emitted = true;
            events.push(AnimationLifecycleEvent::End {
                node_id,
                name: self.name.clone(),
                elapsed_time_sec: 0.0,
            });
        }

        events
    }
}

/// Comprehensive animation coordinator managing active CSS keyframe animations across DOM nodes.
#[derive(Debug, Default, Clone)]
pub struct AnimationManager {
    registry: KeyframeRegistry,
    node_animations: HashMap<u64, Vec<ActiveKeyframeAnimation>>,
}

impl AnimationManager {
    /// Creates a new `AnimationManager` with default Tailwind animation tracks.
    pub fn new() -> Self {
        Self {
            registry: KeyframeRegistry::new(),
            node_animations: HashMap::new(),
        }
    }

    /// Returns a mutable reference to the internal `KeyframeRegistry`.
    pub fn registry_mut(&mut self) -> &mut KeyframeRegistry {
        &mut self.registry
    }

    /// Returns true if there are any active running or forward-filling keyframe animations.
    pub fn has_active_animations(&self) -> bool {
        self.node_animations
            .values()
            .any(|anims| anims.iter().any(|a| !a.is_finished))
    }

    /// Returns true when any running keyframe animation targets a property that participates in
    /// box layout, so the layout tree must be recomputed as the value changes.
    pub fn has_layout_affecting_animations(&self) -> bool {
        self.node_animations.values().any(|list| {
            list.iter().filter(|anim| !anim.is_finished).any(|anim| {
                anim.current_values.keys().any(|property| {
                    matches!(
                        property,
                        TransitionProperty::Width
                            | TransitionProperty::Height
                            | TransitionProperty::Margin
                            | TransitionProperty::Padding
                            | TransitionProperty::BorderWidth
                    )
                })
            })
        })
    }

    /// Returns true if there are any active time-based animations (requiring continuous timer ticks).
    pub fn has_time_based_animations(&self) -> bool {
        self.node_animations.values().any(|list| {
            list.iter()
                .any(|a| !a.is_finished && matches!(a.spec.timeline, AnimationTimeline::Time))
        })
    }

    /// Steps active scroll-driven animations on `node_id` at progress `progress` in [0.0, 1.0].
    pub fn step_node_scroll_progress(
        &mut self,
        node_id: u64,
        progress: f32,
        base_style: &ComputedStyle,
    ) -> Vec<AnimationLifecycleEvent> {
        let mut events = Vec::new();
        if let Some(list) = self.node_animations.get_mut(&node_id) {
            for anim in list.iter_mut() {
                if matches!(
                    anim.spec.timeline,
                    AnimationTimeline::Scroll(..) | AnimationTimeline::View(..)
                ) {
                    events.extend(anim.step_progress(node_id, progress, base_style));
                }
            }
        }
        events
    }

    /// Updates animation instances for `node_id` matching the new `style.animations` list.
    pub fn update_styles(
        &mut self,
        node_id: u64,
        style: &ComputedStyle,
        now_ms: f64,
    ) -> Vec<AnimationLifecycleEvent> {
        let mut events = Vec::new();

        if style.animations.is_empty() {
            if let Some(existing_anims) = self.node_animations.remove(&node_id) {
                for anim in existing_anims {
                    if !anim.end_emitted {
                        events.push(AnimationLifecycleEvent::Cancel {
                            node_id,
                            name: anim.name,
                            elapsed_time_sec: ((now_ms - anim.start_time_ms).max(0.0) / 1000.0)
                                as f32,
                        });
                    }
                }
            }
            return events;
        }

        let active_list = self.node_animations.entry(node_id).or_default();

        // 1. Check for removed animations
        let mut i = 0;
        while i < active_list.len() {
            let active_name = &active_list[i].name;
            if !style
                .animations
                .iter()
                .any(|spec| &spec.name == active_name)
            {
                let removed = active_list.remove(i);
                if !removed.end_emitted {
                    events.push(AnimationLifecycleEvent::Cancel {
                        node_id,
                        name: removed.name,
                        elapsed_time_sec: ((now_ms - removed.start_time_ms).max(0.0) / 1000.0)
                            as f32,
                    });
                }
            } else {
                i += 1;
            }
        }

        // 2. Add or update animations from style.animations
        for spec in &style.animations {
            if spec.name.is_empty() || spec.name == "none" {
                continue;
            }

            if let Some(existing) = active_list.iter_mut().find(|a| a.name == spec.name) {
                // Update play state or iteration count if mutated dynamically
                existing.spec.play_state = spec.play_state;
                existing.spec.iteration_count = spec.iteration_count;
            } else if let Some(track) = self.registry.get(&spec.name).cloned() {
                let mut new_anim = ActiveKeyframeAnimation::new(spec.clone(), track, now_ms);
                events.extend(new_anim.step(node_id, now_ms, style));
                active_list.push(new_anim);
            }
        }

        if active_list.is_empty() {
            self.node_animations.remove(&node_id);
        }

        events
    }

    /// Advances all running keyframe animations to timestamp `now_ms`.
    pub fn advance(
        &mut self,
        now_ms: f64,
        base_styles: &HashMap<u64, ComputedStyle>,
    ) -> Vec<AnimationLifecycleEvent> {
        let mut events = Vec::new();

        for (&node_id, anims) in self.node_animations.iter_mut() {
            let default_style = ComputedStyle::default();
            let base_style = base_styles.get(&node_id).unwrap_or(&default_style);

            for anim in anims.iter_mut() {
                if !anim.is_finished {
                    events.extend(anim.step(node_id, now_ms, base_style));
                }
            }
        }

        // Remove animations that have completed and do not fill forwards
        for anims in self.node_animations.values_mut() {
            anims.retain(|a| {
                !a.is_finished
                    || a.spec.fill_mode == AnimationFillMode::Forwards
                    || a.spec.fill_mode == AnimationFillMode::Both
            });
        }

        self.node_animations.retain(|_, v| !v.is_empty());

        events
    }

    /// Applies all active animation property overrides onto `style` for `node_id`.
    pub fn apply_to_style(&self, node_id: u64, style: &mut ComputedStyle) {
        if let Some(anims) = self.node_animations.get(&node_id) {
            for anim in anims {
                for (prop, val) in &anim.current_values {
                    apply_animatable_value(style, prop, val);
                }
            }
        }
    }

    /// Clears all active animations across all nodes.
    pub fn clear(&mut self) {
        self.node_animations.clear();
    }
}
