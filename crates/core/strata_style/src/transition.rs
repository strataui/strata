//! W3C CSS Transitions Level 1 and Level 2 continuous interpolation engine.
//!
//! Provides dual-buffer style diffing, cubic-Bézier and stepped easing evaluation,
//! smooth retargeting and reversal without visual popping, and lifecycle event tracking.

use std::collections::HashMap;

use crate::types::{
    BorderRadius, BoxShadow, Color, ComputedStyle, Dimension, Display, EdgeInsets, TransformOp,
    TransitionBehavior, TransitionEasing, TransitionProperty, TransitionSpec, Visibility,
};

/// Strongly typed container for any animatable CSS property value.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimatableValue {
    Color(Color),
    Dimension(Dimension),
    Number(f32),
    EdgeInsets(EdgeInsets<f32>),
    BorderRadius(BorderRadius),
    BoxShadow(Vec<BoxShadow>),
    Transform(Vec<TransformOp>),
    ClipPath(crate::effects::ClipPath),
    Display(Display),
    Visibility(Visibility),
    Discrete(String),
}

impl AnimatableValue {
    /// Linearly or discretely interpolates from `self` to `other` at progress `t` in `[0.0, 1.0]`.
    pub fn interpolate(&self, other: &Self, t: f32, behavior: TransitionBehavior) -> Self {
        match (self, other) {
            (Self::Color(c1), Self::Color(c2)) => Self::Color(interpolate_color(*c1, *c2, t)),
            (Self::Number(n1), Self::Number(n2)) => {
                Self::Number(n1 + (n2 - n1) * t.clamp(0.0, 1.0))
            }
            (Self::Dimension(d1), Self::Dimension(d2)) => {
                Self::Dimension(interpolate_dimension(d1, d2, t))
            }
            (Self::EdgeInsets(i1), Self::EdgeInsets(i2)) => {
                Self::EdgeInsets(interpolate_edge_insets(i1, i2, t))
            }
            (Self::BorderRadius(r1), Self::BorderRadius(r2)) => {
                Self::BorderRadius(interpolate_border_radius(r1, r2, t))
            }
            (Self::BoxShadow(s1), Self::BoxShadow(s2)) => {
                Self::BoxShadow(interpolate_box_shadow(s1, s2, t))
            }
            (Self::Transform(t1), Self::Transform(t2)) => {
                Self::Transform(interpolate_transform_ops(t1, t2, t))
            }
            (Self::ClipPath(cp1), Self::ClipPath(cp2)) => {
                Self::ClipPath(crate::effects::interpolate_clip_path(cp1, cp2, t))
            }
            (disc1, disc2) => {
                if behavior == TransitionBehavior::AllowDiscrete {
                    if t < 0.5 {
                        disc1.clone()
                    } else {
                        disc2.clone()
                    }
                } else {
                    disc2.clone()
                }
            }
        }
    }
}

/// Linearly interpolates between two RGBA colors with clamping.
pub fn interpolate_color(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (r1, g1, b1, a1) = from.to_f32();
    let (r2, g2, b2, a2) = to.to_f32();
    Color::from_f32(
        r1 + (r2 - r1) * t,
        g1 + (g2 - g1) * t,
        b1 + (b2 - b1) * t,
        a1 + (a2 - a1) * t,
    )
}

/// Linearly interpolates between two Dimensions if unit types match.
pub fn interpolate_dimension(from: &Dimension, to: &Dimension, t: f32) -> Dimension {
    let t = t.clamp(0.0, 1.0);
    match (from, to) {
        (Dimension::Px(a), Dimension::Px(b)) => Dimension::Px(a + (b - a) * t),
        (Dimension::Percent(a), Dimension::Percent(b)) => Dimension::Percent(a + (b - a) * t),
        (Dimension::Rem(a), Dimension::Rem(b)) => Dimension::Rem(a + (b - a) * t),
        (Dimension::Em(a), Dimension::Em(b)) => Dimension::Em(a + (b - a) * t),
        (Dimension::Vw(a), Dimension::Vw(b)) => Dimension::Vw(a + (b - a) * t),
        (Dimension::Vh(a), Dimension::Vh(b)) => Dimension::Vh(a + (b - a) * t),
        _ => {
            if t < 0.5 {
                *from
            } else {
                *to
            }
        }
    }
}

/// Linearly interpolates 4-edge insets.
pub fn interpolate_edge_insets(
    from: &EdgeInsets<f32>,
    to: &EdgeInsets<f32>,
    t: f32,
) -> EdgeInsets<f32> {
    let t = t.clamp(0.0, 1.0);
    EdgeInsets {
        top: from.top + (to.top - from.top) * t,
        right: from.right + (to.right - from.right) * t,
        bottom: from.bottom + (to.bottom - from.bottom) * t,
        left: from.left + (to.left - from.left) * t,
    }
}

/// Linearly interpolates 4-corner border radius.
pub fn interpolate_border_radius(from: &BorderRadius, to: &BorderRadius, t: f32) -> BorderRadius {
    let t = t.clamp(0.0, 1.0);
    BorderRadius::new(
        from.top_left + (to.top_left - from.top_left) * t,
        from.top_right + (to.top_right - from.top_right) * t,
        from.bottom_right + (to.bottom_right - from.bottom_right) * t,
        from.bottom_left + (to.bottom_left - from.bottom_left) * t,
    )
}

/// Interpolates a list of box shadows per CSS Transitions specification.
pub fn interpolate_box_shadow(from: &[BoxShadow], to: &[BoxShadow], t: f32) -> Vec<BoxShadow> {
    let t = t.clamp(0.0, 1.0);
    let max_len = from.len().max(to.len());
    let mut result = Vec::with_capacity(max_len);

    for i in 0..max_len {
        let default_shadow = BoxShadow {
            offset_x: 0.0,
            offset_y: 0.0,
            blur_radius: 0.0,
            spread_radius: 0.0,
            color: Color::TRANSPARENT,
            inset: false,
        };
        let f = from.get(i).unwrap_or(&default_shadow);
        let tr = to.get(i).unwrap_or(&default_shadow);

        result.push(BoxShadow {
            offset_x: f.offset_x + (tr.offset_x - f.offset_x) * t,
            offset_y: f.offset_y + (tr.offset_y - f.offset_y) * t,
            blur_radius: (f.blur_radius + (tr.blur_radius - f.blur_radius) * t).max(0.0),
            spread_radius: f.spread_radius + (tr.spread_radius - f.spread_radius) * t,
            color: interpolate_color(f.color, tr.color, t),
            inset: if t < 0.5 { f.inset } else { tr.inset },
        });
    }

    result
}

/// Interpolates between two lists of transform operations per W3C CSS Transforms specification.
pub fn interpolate_transform_ops(
    from: &[TransformOp],
    to: &[TransformOp],
    t: f32,
) -> Vec<TransformOp> {
    let t = t.clamp(0.0, 1.0);
    if from.is_empty() && to.is_empty() {
        return Vec::new();
    }
    if from.is_empty() {
        // Expand from with identity operations matching to
        return to
            .iter()
            .map(|op| interpolate_single_transform_op(&op.to_identity(), op, t))
            .collect();
    }
    if to.is_empty() {
        // Expand to with identity operations matching from
        return from
            .iter()
            .map(|op| interpolate_single_transform_op(op, &op.to_identity(), t))
            .collect();
    }
    if from.len() == to.len() {
        let mut result = Vec::with_capacity(from.len());
        for (op1, op2) in from.iter().zip(to.iter()) {
            result.push(interpolate_single_transform_op(op1, op2, t));
        }
        return result;
    }
    // If lengths differ, compose each list into a combined matrix and interpolate decomposed parameters
    let mut mat1 = strata_primitives::geom::Matrix4x4::identity();
    for op in from {
        mat1 = mat1.multiply(&op.to_matrix(100.0, 100.0));
    }
    let mut mat2 = strata_primitives::geom::Matrix4x4::identity();
    for op in to {
        mat2 = mat2.multiply(&op.to_matrix(100.0, 100.0));
    }
    let m1_2d = [mat1.m[0], mat1.m[1], mat1.m[4], mat1.m[5], mat1.m[12], mat1.m[13]];
    let m2_2d = [mat2.m[0], mat2.m[1], mat2.m[4], mat2.m[5], mat2.m[12], mat2.m[13]];
    if let (Some(d1), Some(d2)) = (
        crate::matrix_decomp::Decomposed2d::from_matrix(m1_2d),
        crate::matrix_decomp::Decomposed2d::from_matrix(m2_2d),
    ) {
        let interpolated = d1.interpolate(&d2, t);
        vec![TransformOp::Matrix(interpolated.to_matrix())]
    } else if t < 0.5 {
        from.to_vec()
    } else {
        to.to_vec()
    }
}

/// Interpolates two individual transform operations if compatible.
pub fn interpolate_single_transform_op(
    from: &TransformOp,
    to: &TransformOp,
    t: f32,
) -> TransformOp {
    let t = t.clamp(0.0, 1.0);
    match (from, to) {
        (TransformOp::Translate(x1, y1), TransformOp::Translate(x2, y2)) => TransformOp::Translate(
            interpolate_dimension(x1, x2, t),
            interpolate_dimension(y1, y2, t),
        ),
        (TransformOp::Translate3d(x1, y1, z1), TransformOp::Translate3d(x2, y2, z2)) => {
            TransformOp::Translate3d(
                interpolate_dimension(x1, x2, t),
                interpolate_dimension(y1, y2, t),
                interpolate_dimension(z1, z2, t),
            )
        }
        (TransformOp::Scale(sx1, sy1), TransformOp::Scale(sx2, sy2)) => {
            TransformOp::Scale(sx1 + (sx2 - sx1) * t, sy1 + (sy2 - sy1) * t)
        }
        (TransformOp::Scale3d(sx1, sy1, sz1), TransformOp::Scale3d(sx2, sy2, sz2)) => {
            TransformOp::Scale3d(
                sx1 + (sx2 - sx1) * t,
                sy1 + (sy2 - sy1) * t,
                sz1 + (sz2 - sz1) * t,
            )
        }
        (TransformOp::Rotate(r1), TransformOp::Rotate(r2)) => {
            TransformOp::Rotate(r1 + (r2 - r1) * t)
        }
        (TransformOp::RotateX(r1), TransformOp::RotateX(r2)) => {
            TransformOp::RotateX(r1 + (r2 - r1) * t)
        }
        (TransformOp::RotateY(r1), TransformOp::RotateY(r2)) => {
            TransformOp::RotateY(r1 + (r2 - r1) * t)
        }
        (TransformOp::RotateZ(r1), TransformOp::RotateZ(r2)) => {
            TransformOp::RotateZ(r1 + (r2 - r1) * t)
        }
        (TransformOp::Rotate3d(x1, y1, z1, r1), TransformOp::Rotate3d(x2, y2, z2, r2)) => {
            if (x1 - x2).abs() < 1e-4 && (y1 - y2).abs() < 1e-4 && (z1 - z2).abs() < 1e-4 {
                TransformOp::Rotate3d(*x1, *y1, *z1, r1 + (r2 - r1) * t)
            } else if t < 0.5 {
                from.clone()
            } else {
                to.clone()
            }
        }
        (TransformOp::Skew(ax1, ay1), TransformOp::Skew(ax2, ay2)) => {
            TransformOp::Skew(ax1 + (ax2 - ax1) * t, ay1 + (ay2 - ay1) * t)
        }
        (TransformOp::Perspective(d1), TransformOp::Perspective(d2)) => {
            TransformOp::Perspective(d1 + (d2 - d1) * t)
        }
        (TransformOp::Matrix(m1), TransformOp::Matrix(m2)) => {
            if let (Some(d1), Some(d2)) = (
                crate::matrix_decomp::Decomposed2d::from_matrix(*m1),
                crate::matrix_decomp::Decomposed2d::from_matrix(*m2),
            ) {
                let interpolated = d1.interpolate(&d2, t);
                TransformOp::Matrix(interpolated.to_matrix())
            } else {
                let mut out = [0.0f32; 6];
                for i in 0..6 {
                    out[i] = m1[i] + (m2[i] - m1[i]) * t;
                }
                TransformOp::Matrix(out)
            }
        }
        (TransformOp::Matrix3d(m1), TransformOp::Matrix3d(m2)) => {
            let mut out = [0.0f32; 16];
            for i in 0..16 {
                out[i] = m1[i] + (m2[i] - m1[i]) * t;
            }
            TransformOp::Matrix3d(out)
        }
        _ => {
            let mat1 = from.to_matrix(100.0, 100.0);
            let mat2 = to.to_matrix(100.0, 100.0);
            let m1_2d = [mat1.m[0], mat1.m[1], mat1.m[4], mat1.m[5], mat1.m[12], mat1.m[13]];
            let m2_2d = [mat2.m[0], mat2.m[1], mat2.m[4], mat2.m[5], mat2.m[12], mat2.m[13]];
            if let (Some(d1), Some(d2)) = (
                crate::matrix_decomp::Decomposed2d::from_matrix(m1_2d),
                crate::matrix_decomp::Decomposed2d::from_matrix(m2_2d),
            ) {
                let interpolated = d1.interpolate(&d2, t);
                TransformOp::Matrix(interpolated.to_matrix())
            } else if t < 0.5 {
                from.clone()
            } else {
                to.clone()
            }
        }
    }
}

/// Lifecycle notification events dispatched during CSS transition execution.
#[derive(Debug, Clone, PartialEq)]
pub enum TransitionLifecycleEvent {
    /// Fired when transition is created (start of delay).
    Run {
        node_id: u64,
        property: String,
        elapsed_time_sec: f32,
    },
    /// Fired when transition enters active phase (delay has elapsed).
    Start {
        node_id: u64,
        property: String,
        elapsed_time_sec: f32,
    },
    /// Fired when transition completes its full duration.
    End {
        node_id: u64,
        property: String,
        elapsed_time_sec: f32,
    },
    /// Fired when transition is aborted mid-flight before completion.
    Cancel {
        node_id: u64,
        property: String,
        elapsed_time_sec: f32,
    },
}

/// An actively animating CSS transition on an individual property of a DOM node.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveTransition {
    pub property: TransitionProperty,
    pub start_time_ms: f64,
    pub duration_ms: f32,
    pub delay_ms: f32,
    pub easing: TransitionEasing,
    pub behavior: TransitionBehavior,
    pub from_value: AnimatableValue,
    pub to_value: AnimatableValue,
    pub current_value: AnimatableValue,
    pub run_emitted: bool,
    pub start_emitted: bool,
    pub end_emitted: bool,
    pub reversing_factor: f32,
}

impl ActiveTransition {
    /// Creates a new `ActiveTransition` starting from `from_value` targeting `to_value`.
    pub fn new(
        property: TransitionProperty,
        spec: &TransitionSpec,
        from_value: AnimatableValue,
        to_value: AnimatableValue,
        start_time_ms: f64,
    ) -> Self {
        let current_value = from_value.clone();
        Self {
            property,
            start_time_ms,
            duration_ms: spec.duration_ms,
            delay_ms: spec.delay_ms,
            easing: spec.timing_function,
            behavior: spec.behavior,
            from_value,
            to_value,
            current_value,
            run_emitted: false,
            start_emitted: false,
            end_emitted: false,
            reversing_factor: 1.0,
        }
    }

    /// Evaluates the transition at timestamp `now_ms` and updates `current_value`.
    /// Returns any lifecycle events triggered during this evaluation step.
    pub fn step(&mut self, node_id: u64, now_ms: f64) -> Vec<TransitionLifecycleEvent> {
        let mut events = Vec::new();
        let elapsed_ms = (now_ms - self.start_time_ms) as f32;

        // 1. Check Run event
        if !self.run_emitted {
            self.run_emitted = true;
            events.push(TransitionLifecycleEvent::Run {
                node_id,
                property: self.property.as_str().to_string(),
                elapsed_time_sec: 0.0,
            });
        }

        // 2. Check Delay phase
        if elapsed_ms < self.delay_ms {
            self.current_value = self.from_value.clone();
            return events;
        }

        // 3. Active execution phase
        if !self.start_emitted {
            self.start_emitted = true;
            events.push(TransitionLifecycleEvent::Start {
                node_id,
                property: self.property.as_str().to_string(),
                elapsed_time_sec: (elapsed_ms - self.delay_ms).max(0.0) / 1000.0,
            });
        }

        let active_ms = elapsed_ms - self.delay_ms;
        let raw_progress = if self.duration_ms > 0.0 {
            (active_ms / self.duration_ms).clamp(0.0, 1.0)
        } else {
            1.0
        };

        let eased_progress = self.easing.evaluate(raw_progress);
        self.current_value =
            self.from_value
                .interpolate(&self.to_value, eased_progress, self.behavior);

        // 4. Completion check
        if raw_progress >= 1.0 && !self.end_emitted {
            self.end_emitted = true;
            events.push(TransitionLifecycleEvent::End {
                node_id,
                property: self.property.as_str().to_string(),
                elapsed_time_sec: self.duration_ms / 1000.0,
            });
        }

        events
    }

    /// Checks if this transition has fully completed its duration and emitted its End event.
    pub fn is_finished(&self) -> bool {
        self.end_emitted
    }
}

/// Extracts an animatable property value from a `ComputedStyle`.
pub fn extract_animatable_value(
    style: &ComputedStyle,
    prop: &TransitionProperty,
) -> Option<AnimatableValue> {
    match prop {
        TransitionProperty::Color => Some(AnimatableValue::Color(style.color)),
        TransitionProperty::BackgroundColor => style.background_color.map(AnimatableValue::Color),
        TransitionProperty::Opacity => Some(AnimatableValue::Number(style.opacity)),
        TransitionProperty::Width => Some(AnimatableValue::Dimension(style.width)),
        TransitionProperty::Height => Some(AnimatableValue::Dimension(style.height)),
        TransitionProperty::Margin => Some(AnimatableValue::EdgeInsets(style.margin)),
        TransitionProperty::Padding => Some(AnimatableValue::EdgeInsets(style.padding)),
        TransitionProperty::BorderColor => style.border_color.map(AnimatableValue::Color),
        TransitionProperty::BorderWidth => Some(AnimatableValue::EdgeInsets(style.border_width)),
        TransitionProperty::BorderRadius => {
            Some(AnimatableValue::BorderRadius(style.border_radius))
        }
        TransitionProperty::BoxShadow => Some(AnimatableValue::BoxShadow(style.box_shadow.clone())),
        TransitionProperty::Transform => Some(AnimatableValue::Transform(style.transform.clone())),
        TransitionProperty::ClipPath => style.clip_path.clone().map(AnimatableValue::ClipPath),
        _ => None,
    }
}

/// Applies an interpolated animatable property value onto a mutable `ComputedStyle`.
pub fn apply_animatable_value(
    style: &mut ComputedStyle,
    prop: &TransitionProperty,
    val: &AnimatableValue,
) {
    match (prop, val) {
        (TransitionProperty::Color, AnimatableValue::Color(c)) => style.color = *c,
        (TransitionProperty::BackgroundColor, AnimatableValue::Color(c)) => {
            style.background_color = Some(*c);
        }
        (TransitionProperty::Opacity, AnimatableValue::Number(n)) => style.opacity = *n,
        (TransitionProperty::Width, AnimatableValue::Dimension(d)) => style.width = *d,
        (TransitionProperty::Height, AnimatableValue::Dimension(d)) => style.height = *d,
        (TransitionProperty::Margin, AnimatableValue::EdgeInsets(m)) => style.margin = *m,
        (TransitionProperty::Padding, AnimatableValue::EdgeInsets(p)) => style.padding = *p,
        (TransitionProperty::BorderColor, AnimatableValue::Color(c)) => {
            style.border_color = Some(*c);
        }
        (TransitionProperty::BorderWidth, AnimatableValue::EdgeInsets(w)) => {
            style.border_width = *w;
        }
        (TransitionProperty::BorderRadius, AnimatableValue::BorderRadius(r)) => {
            style.border_radius = *r;
        }
        (TransitionProperty::BoxShadow, AnimatableValue::BoxShadow(s)) => {
            style.box_shadow = s.clone();
        }
        (TransitionProperty::Transform, AnimatableValue::Transform(t)) => {
            style.transform = t.clone();
        }
        (TransitionProperty::ClipPath, AnimatableValue::ClipPath(cp)) => {
            style.clip_path = Some(cp.clone());
        }
        _ => {}
    }
}

/// Finds the most specific `TransitionSpec` matching a target property.
pub fn find_transition_spec<'a>(
    specs: &'a [TransitionSpec],
    prop: &TransitionProperty,
) -> Option<&'a TransitionSpec> {
    // 1. Direct match on property
    if let Some(spec) = specs.iter().find(|s| &s.property == prop) {
        return Some(spec);
    }
    // 2. Fallback to 'all'
    if let Some(spec) = specs.iter().find(|s| s.property == TransitionProperty::All) {
        return Some(spec);
    }
    None
}

/// Comprehensive transition coordinator managing active transitions across DOM nodes.
#[derive(Debug, Default, Clone)]
pub struct TransitionManager {
    node_transitions: HashMap<u64, Vec<ActiveTransition>>,
}

impl TransitionManager {
    /// Creates a new, empty `TransitionManager`.
    pub fn new() -> Self {
        Self {
            node_transitions: HashMap::new(),
        }
    }

    /// Returns true if there are any active running transitions.
    pub fn has_active_transitions(&self) -> bool {
        self.node_transitions.values().any(|v| !v.is_empty())
    }

    /// Returns true when any running transition targets a property that participates in
    /// box layout, so the layout tree must be recomputed as the value changes.
    pub fn has_layout_affecting_transitions(&self) -> bool {
        self.node_transitions.values().any(|list| {
            list.iter().any(|transition| {
                matches!(
                    transition.property,
                    TransitionProperty::Width
                        | TransitionProperty::Height
                        | TransitionProperty::Margin
                        | TransitionProperty::Padding
                        | TransitionProperty::BorderWidth
                )
            })
        })
    }

    /// Diffs the previous computed style against the new style for `node_id`,
    /// initiating, retargeting, or reversing transitions per W3C specification.
    pub fn update_styles(
        &mut self,
        node_id: u64,
        old_style: &ComputedStyle,
        new_style: &ComputedStyle,
        now_ms: f64,
    ) -> Vec<TransitionLifecycleEvent> {
        let mut events = Vec::new();
        if new_style.transitions.is_empty() {
            return events;
        }

        // List of all primary animatable properties to check
        let animatable_props = [
            TransitionProperty::Color,
            TransitionProperty::BackgroundColor,
            TransitionProperty::Opacity,
            TransitionProperty::Width,
            TransitionProperty::Height,
            TransitionProperty::Margin,
            TransitionProperty::Padding,
            TransitionProperty::BorderColor,
            TransitionProperty::BorderWidth,
            TransitionProperty::BorderRadius,
            TransitionProperty::BoxShadow,
            TransitionProperty::Transform,
        ];

        let active_list = self.node_transitions.entry(node_id).or_default();

        for prop in animatable_props {
            let old_val = match extract_animatable_value(old_style, &prop) {
                Some(v) => v,
                None => continue,
            };
            let new_val = match extract_animatable_value(new_style, &prop) {
                Some(v) => v,
                None => continue,
            };

            // Check if the property target value actually changed
            if old_val == new_val {
                continue;
            }

            let spec = match find_transition_spec(&new_style.transitions, &prop) {
                Some(s) => s,
                None => continue,
            };

            if spec.property == TransitionProperty::None {
                continue;
            }

            if spec.duration_ms <= 0.0 && spec.delay_ms <= 0.0 {
                continue;
            }

            // Check if there is already an active transition for this property
            let existing_idx = active_list.iter().position(|t| t.property == prop);

            if let Some(idx) = existing_idx {
                let existing = &mut active_list[idx];

                // If targeting same value, no-op
                if existing.to_value == new_val {
                    continue;
                }

                // Interrupted transition cancellation event
                if !existing.end_emitted {
                    events.push(TransitionLifecycleEvent::Cancel {
                        node_id,
                        property: prop.as_str().to_string(),
                        elapsed_time_sec: ((now_ms - existing.start_time_ms) as f32).max(0.0)
                            / 1000.0,
                    });
                }

                // Smooth retargeting / reversal
                let from_val = existing.current_value.clone();
                let is_reversal = existing.from_value == new_val;

                let mut new_transition =
                    ActiveTransition::new(prop.clone(), spec, from_val, new_val, now_ms);

                if is_reversal {
                    let elapsed =
                        ((now_ms - existing.start_time_ms) as f32 - existing.delay_ms).max(0.0);
                    let p = if existing.duration_ms > 0.0 {
                        (elapsed / existing.duration_ms).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    let new_reversing_factor = p * existing.reversing_factor;
                    new_transition.reversing_factor = new_reversing_factor;
                    new_transition.duration_ms = spec.duration_ms * new_reversing_factor;
                }

                events.extend(new_transition.step(node_id, now_ms));
                active_list[idx] = new_transition;
            } else {
                // New transition starting from old_val
                let mut new_transition =
                    ActiveTransition::new(prop.clone(), spec, old_val, new_val, now_ms);
                events.extend(new_transition.step(node_id, now_ms));
                active_list.push(new_transition);
            }
        }

        // Clean up empty entries
        if active_list.is_empty() {
            self.node_transitions.remove(&node_id);
        }

        events
    }

    /// Advances all running transitions to timestamp `now_ms` and generates lifecycle events.
    pub fn advance(&mut self, now_ms: f64) -> Vec<TransitionLifecycleEvent> {
        let mut events = Vec::new();

        for (&node_id, transitions) in self.node_transitions.iter_mut() {
            for trans in transitions.iter_mut() {
                if !trans.is_finished() {
                    events.extend(trans.step(node_id, now_ms));
                }
            }
        }

        // Remove finished transitions that have emitted End
        for transitions in self.node_transitions.values_mut() {
            transitions.retain(|t| !t.is_finished());
        }

        self.node_transitions.retain(|_, v| !v.is_empty());

        events
    }

    /// Applies all active transition values for `node_id` onto the specified `ComputedStyle`.
    pub fn apply_to_style(&self, node_id: u64, style: &mut ComputedStyle) {
        if let Some(transitions) = self.node_transitions.get(&node_id) {
            for trans in transitions {
                apply_animatable_value(style, &trans.property, &trans.current_value);
            }
        }
    }

    /// Clears all active transitions.
    pub fn clear(&mut self) {
        self.node_transitions.clear();
    }
}
