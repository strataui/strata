//! Spatial acceleration index for hit-testing, point queries, and EntityId resolution.

use std::collections::HashMap;
use std::rc::Rc;
use strata_dom::{Event, EventType, NodeBlueprint};
use strata_layout::tree::LayoutNode;
use strata_layout::Rect;
use strata_primitives::EntityId;
use strata_style::types::{Display, Float, Overflow, PointerEvents, Position, Visibility};

use crate::runtime::AppRuntime;

/// An individual spatial entry for an entity in the layout hierarchy.
#[derive(Clone)]
pub struct SpatialEntry {
    pub entity_id: EntityId,
    pub bounds: Rect,
    pub event_listeners: Vec<(EventType, Rc<dyn Fn(Event)>)>,
    pub z_index: i32,
    pub order: usize,
    root_z_key: i64,
    clip_bounds: Option<Rect>,
    hittable: bool,
}

impl SpatialEntry {
    #[inline]
    fn contains_clipped_point(&self, px: f32, py: f32) -> bool {
        if !self.hittable || !self.bounds.contains_point(px, py) {
            return false;
        }
        match self.clip_bounds {
            Some(clip) => clip.width >= 0.0 && clip.height >= 0.0 && clip.contains_point(px, py),
            None => true,
        }
    }
}

impl std::fmt::Debug for SpatialEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpatialEntry")
            .field("entity_id", &self.entity_id)
            .field("bounds", &self.bounds)
            .field("listeners_count", &self.event_listeners.len())
            .field("z_index", &self.z_index)
            .field("order", &self.order)
            .finish()
    }
}

#[inline]
fn intersect_clip(existing: Option<Rect>, next: Rect) -> Option<Rect> {
    match existing {
        None => Some(next),
        Some(prev) => {
            if prev.width < 0.0 || prev.height < 0.0 {
                return Some(prev);
            }
            let x0 = prev.x.max(next.x);
            let y0 = prev.y.max(next.y);
            let x1 = (prev.x + prev.width).min(next.x + next.width);
            let y1 = (prev.y + prev.height).min(next.y + next.height);
            if x1 < x0 || y1 < y0 {
                Some(Rect::new(0.0, 0.0, -1.0, -1.0))
            } else {
                Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
            }
        }
    }
}

/// Accelerated spatial index maintaining entity bounding boxes and event listeners.
#[derive(Default, Clone, Debug)]
pub struct SpatialIndex {
    entries: Vec<SpatialEntry>,
    entity_to_index: HashMap<EntityId, usize>,
}

impl SpatialIndex {
    /// Creates a new empty spatial index.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            entity_to_index: HashMap::new(),
        }
    }

    /// Clears and rebuilds the spatial index from the active blueprint and computed layout hierarchy.
    pub fn rebuild(&mut self, blueprint: &NodeBlueprint, layout: &LayoutNode) {
        self.entries.clear();
        self.entity_to_index.clear();

        let mut order = 0;
        let root_z = layout.style.z_index.unwrap_or(0);
        Self::collect_stacking_context(
            blueprint,
            layout,
            &mut self.entries,
            &mut order,
            root_z,
            i64::MIN,
            true,
            None,
            0,
        );

        for (idx, entry) in self.entries.iter().enumerate() {
            if entry.entity_id.is_valid() {
                self.entity_to_index.insert(entry.entity_id, idx);
            }
        }
    }

    fn emit_node_entry(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        out: &mut Vec<SpatialEntry>,
        order: &mut usize,
        effective_z: i32,
        root_z_key: i64,
        active_clip: Option<Rect>,
    ) {
        let entity_id = layout.entity_id;
        let bounds = layout.geometry.border_box;
        let mut listeners = Vec::new();

        if let NodeBlueprint::Element { attributes, .. } = blueprint {
            for attr in attributes {
                if let strata_dom::Attribute::EventListener { event, handler } = attr {
                    listeners.push((*event, Rc::clone(handler)));
                }
            }
        }

        if entity_id.is_valid() || !listeners.is_empty() {
            let hittable = !matches!(layout.style.visibility, Visibility::Hidden)
                && !matches!(layout.style.pointer_events, PointerEvents::None);
            let entry = SpatialEntry {
                entity_id,
                bounds,
                event_listeners: listeners,
                z_index: effective_z,
                order: *order,
                root_z_key,
                clip_bounds: active_clip,
                hittable,
            };
            *order += 1;
            out.push(entry);
        }
    }

    fn paired_children_with_fallback<'a>(
        node_bp: &'a NodeBlueprint,
        node_layout: &'a LayoutNode,
        fallback_bp: &'a NodeBlueprint,
    ) -> Vec<(&'a NodeBlueprint, &'a LayoutNode)> {
        let mut flat_children = Vec::new();
        match node_bp {
            NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                for c in children {
                    AppRuntime::flatten_blueprints(c, &mut flat_children);
                }
            }
            _ => {}
        }

        let mut out = Vec::with_capacity(node_layout.children.len());
        for (idx, child_layout) in node_layout.children.iter().enumerate() {
            let resolved_bp = if child_layout.entity_id.is_valid() {
                if let Some(&cand) = flat_children.get(idx) {
                    if cand.entity_id() == Some(child_layout.entity_id) {
                        cand
                    } else {
                        node_bp
                            .find_by_entity_id(child_layout.entity_id)
                            .unwrap_or(cand)
                    }
                } else {
                    node_bp
                        .find_by_entity_id(child_layout.entity_id)
                        .unwrap_or(fallback_bp)
                }
            } else {
                flat_children.get(idx).copied().unwrap_or(fallback_bp)
            };
            out.push((resolved_bp, child_layout));
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_hoisted_participants<'a>(
        bp: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        fallback_bp: &'a NodeBlueprint,
        negative_z: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode)>,
        auto_or_zero_z: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode, bool)>,
        positive_z: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode)>,
        depth: usize,
    ) {
        if depth >= 256 {
            return;
        }
        for (child_bp, child_layout) in
            Self::paired_children_with_fallback(bp, layout, fallback_bp)
        {
            if child_layout.style.display == Display::None {
                continue;
            }
            if AppRuntime::establishes_stacking_context(child_layout, false) {
                let z = child_layout.style.z_index.unwrap_or(0);
                if z < 0 {
                    negative_z.push((child_bp, child_layout));
                } else if z > 0 {
                    positive_z.push((child_bp, child_layout));
                } else {
                    auto_or_zero_z.push((child_bp, child_layout, true));
                }
            } else {
                if child_layout.style.position != Position::Static {
                    auto_or_zero_z.push((child_bp, child_layout, false));
                }
                Self::collect_hoisted_participants(
                    child_bp,
                    child_layout,
                    fallback_bp,
                    negative_z,
                    auto_or_zero_z,
                    positive_z,
                    depth + 1,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_direct_in_flow_children(
        bp: &NodeBlueprint,
        layout: &LayoutNode,
        out: &mut Vec<SpatialEntry>,
        order: &mut usize,
        inherited_z: i32,
        root_z_key: i64,
        active_clip: Option<Rect>,
        depth: usize,
    ) {
        let fallback_bp = NodeBlueprint::Empty;
        let paired = Self::paired_children_with_fallback(bp, layout, &fallback_bp);
        if paired.is_empty() {
            return;
        }

        let is_in_flow_non_sc = |c: &LayoutNode| {
            c.style.display != Display::None
                && !AppRuntime::establishes_stacking_context(c, false)
                && c.style.position == Position::Static
        };
        let is_inline_level = |c: &LayoutNode| {
            matches!(
                c.style.display,
                Display::Inline | Display::InlineBlock | Display::InlineFlex | Display::InlineGrid
            )
        };

        // Layer 3: In-flow blocks
        for &(child_bp, child_layout) in &paired {
            if is_in_flow_non_sc(child_layout)
                && child_layout.style.float == Float::None
                && !is_inline_level(child_layout)
            {
                Self::collect_in_flow_non_hoisted(
                    child_bp,
                    child_layout,
                    out,
                    order,
                    inherited_z,
                    root_z_key,
                    active_clip,
                    depth + 1,
                );
            }
        }

        // Layer 4: Floats
        for &(child_bp, child_layout) in &paired {
            if is_in_flow_non_sc(child_layout) && child_layout.style.float != Float::None {
                Self::collect_in_flow_non_hoisted(
                    child_bp,
                    child_layout,
                    out,
                    order,
                    inherited_z,
                    root_z_key,
                    active_clip,
                    depth + 1,
                );
            }
        }

        // Layer 5: In-flow inlines
        for &(child_bp, child_layout) in &paired {
            if is_in_flow_non_sc(child_layout)
                && child_layout.style.float == Float::None
                && is_inline_level(child_layout)
            {
                Self::collect_in_flow_non_hoisted(
                    child_bp,
                    child_layout,
                    out,
                    order,
                    inherited_z,
                    root_z_key,
                    active_clip,
                    depth + 1,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_in_flow_non_hoisted(
        bp: &NodeBlueprint,
        layout: &LayoutNode,
        out: &mut Vec<SpatialEntry>,
        order: &mut usize,
        inherited_z: i32,
        root_z_key: i64,
        active_clip: Option<Rect>,
        depth: usize,
    ) {
        if depth >= 256 || layout.style.display == Display::None {
            return;
        }
        let effective_z = layout.style.z_index.unwrap_or(inherited_z);
        Self::emit_node_entry(bp, layout, out, order, effective_z, root_z_key, active_clip);
        Self::collect_direct_in_flow_children(
            bp,
            layout,
            out,
            order,
            effective_z,
            root_z_key,
            active_clip,
            depth,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_stacking_context(
        bp: &NodeBlueprint,
        layout: &LayoutNode,
        out: &mut Vec<SpatialEntry>,
        order: &mut usize,
        effective_z: i32,
        self_root_z_key: i64,
        is_root: bool,
        parent_clip: Option<Rect>,
        depth: usize,
    ) {
        if depth >= 256 || layout.style.display == Display::None {
            return;
        }

        // Layer 1: Stacking context root element itself
        Self::emit_node_entry(
            bp,
            layout,
            out,
            order,
            effective_z,
            self_root_z_key,
            parent_clip,
        );

        let child_clip = if layout.style.overflow_x != Overflow::Visible
            || layout.style.overflow_y != Overflow::Visible
        {
            intersect_clip(parent_clip, layout.geometry.border_box)
        } else {
            parent_clip
        };

        let fallback_bp = NodeBlueprint::Empty;
        let mut negative_z = Vec::new();
        let mut auto_or_zero_z = Vec::new();
        let mut positive_z = Vec::new();
        Self::collect_hoisted_participants(
            bp,
            layout,
            &fallback_bp,
            &mut negative_z,
            &mut auto_or_zero_z,
            &mut positive_z,
            depth + 1,
        );

        negative_z.sort_by_key(|(_, l)| l.style.z_index.unwrap_or(0));
        positive_z.sort_by_key(|(_, l)| l.style.z_index.unwrap_or(0));

        // Layer 2: Child stacking contexts with negative z-index (ascending)
        for (c_bp, c_layout) in negative_z {
            let cz = c_layout.style.z_index.unwrap_or(0);
            let child_root_key = if is_root { cz as i64 } else { self_root_z_key };
            Self::collect_stacking_context(
                c_bp,
                c_layout,
                out,
                order,
                cz,
                child_root_key,
                false,
                child_clip,
                depth + 1,
            );
        }

        // Layers 3, 4, 5: In-flow blocks, floats, and inlines
        let normal_root_key = if is_root { 0 } else { self_root_z_key };
        Self::collect_direct_in_flow_children(
            bp,
            layout,
            out,
            order,
            effective_z,
            normal_root_key,
            child_clip,
            depth,
        );

        // Layer 6: Positioned descendants with z-index: auto and child stacking contexts with z-index: 0
        for (c_bp, c_layout, is_sc) in auto_or_zero_z {
            if is_sc {
                let cz = c_layout.style.z_index.unwrap_or(effective_z);
                Self::collect_stacking_context(
                    c_bp,
                    c_layout,
                    out,
                    order,
                    cz,
                    normal_root_key,
                    false,
                    child_clip,
                    depth + 1,
                );
            } else {
                Self::collect_in_flow_non_hoisted(
                    c_bp,
                    c_layout,
                    out,
                    order,
                    effective_z,
                    normal_root_key,
                    child_clip,
                    depth + 1,
                );
            }
        }

        // Layer 7: Child stacking contexts with positive z-index (ascending)
        for (c_bp, c_layout) in positive_z {
            let cz = c_layout.style.z_index.unwrap_or(0);
            let child_root_key = if is_root { cz as i64 } else { self_root_z_key };
            Self::collect_stacking_context(
                c_bp,
                c_layout,
                out,
                order,
                cz,
                child_root_key,
                false,
                child_clip,
                depth + 1,
            );
        }
    }

    /// Queries the front-most / deepest EntityId at point (px, py), honoring stacking `z_index` and paint order.
    pub fn query_entity_at_point(&self, px: f32, py: f32) -> Option<EntityId> {
        let mut best: Option<&SpatialEntry> = None;
        for entry in self.entries.iter().rev() {
            if entry.entity_id.is_valid() && entry.contains_clipped_point(px, py) {
                match best {
                    None => best = Some(entry),
                    Some(cur) => {
                        if (entry.root_z_key, entry.order) > (cur.root_z_key, cur.order) {
                            best = Some(entry);
                        }
                    }
                }
            }
        }
        best.map(|e| e.entity_id)
    }

    /// Queries the front-most event listener registered for `event_type` at point (px, py),
    /// honoring stacking `z_index` and paint order.
    pub fn query_event_at_point(
        &self,
        px: f32,
        py: f32,
        event_type: EventType,
    ) -> Option<(EntityId, Rc<dyn Fn(Event)>, Rect)> {
        let mut best: Option<(&SpatialEntry, &Rc<dyn Fn(Event)>)> = None;
        for entry in self.entries.iter().rev() {
            if entry.contains_clipped_point(px, py) {
                for (ev, handler) in &entry.event_listeners {
                    if *ev == event_type {
                        match best {
                            None => best = Some((entry, handler)),
                            Some((cur, _)) => {
                                if (entry.root_z_key, entry.order) > (cur.root_z_key, cur.order) {
                                    best = Some((entry, handler));
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
        best.map(|(entry, handler)| (entry.entity_id, Rc::clone(handler), entry.bounds))
    }

    /// Returns the bounding box of an entity if present in the index.
    pub fn get_bounds(&self, entity_id: EntityId) -> Option<Rect> {
        let idx = self.entity_to_index.get(&entity_id)?;
        Some(self.entries[*idx].bounds)
    }

    /// Returns the total number of indexed spatial entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the index has no spatial entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
