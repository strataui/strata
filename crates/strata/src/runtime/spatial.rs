//! Spatial acceleration index for hit-testing, point queries, and EntityId resolution.

use std::collections::HashMap;
use std::rc::Rc;
use strata_dom::{Event, EventType, NodeBlueprint};
use strata_layout::tree::LayoutNode;
use strata_layout::Rect;
use strata_primitives::EntityId;

/// An individual spatial entry for an entity in the layout hierarchy.
#[derive(Clone)]
pub struct SpatialEntry {
    pub entity_id: EntityId,
    pub bounds: Rect,
    pub event_listeners: Vec<(EventType, Rc<dyn Fn(Event)>)>,
    pub z_index: i32,
    pub order: usize,
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
        Self::collect_entries(blueprint, layout, &mut self.entries, &mut order);

        for (idx, entry) in self.entries.iter().enumerate() {
            if entry.entity_id.is_valid() {
                self.entity_to_index.insert(entry.entity_id, idx);
            }
        }
    }

    fn collect_entries(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        out: &mut Vec<SpatialEntry>,
        order: &mut usize,
    ) {
        let entity_id = layout.entity_id;
        let bounds = layout.geometry.border_box;
        let mut listeners = Vec::new();

        // Extract event listeners from blueprint if it matches this entity
        if let NodeBlueprint::Element { attributes, .. } = blueprint {
            for attr in attributes {
                if let strata_dom::Attribute::EventListener { event, handler } = attr {
                    listeners.push((*event, Rc::clone(handler)));
                }
            }
        }

        if entity_id.is_valid() || !listeners.is_empty() {
            let entry = SpatialEntry {
                entity_id,
                bounds,
                event_listeners: listeners,
                z_index: layout.style.z_index.unwrap_or(0),
                order: *order,
            };
            *order += 1;
            out.push(entry);
        }

        // Recurse children
        for child_layout in &layout.children {
            let child_bp = if child_layout.entity_id.is_valid() {
                blueprint.find_by_entity_id(child_layout.entity_id)
            } else {
                None
            };
            let fallback_bp = NodeBlueprint::Empty;
            let bp_ref = child_bp.unwrap_or(&fallback_bp);
            Self::collect_entries(bp_ref, child_layout, out, order);
        }
    }

    /// Queries the front-most / deepest EntityId at point (px, py).
    pub fn query_entity_at_point(&self, px: f32, py: f32) -> Option<EntityId> {
        // Search in reverse document order (front-most children first)
        for entry in self.entries.iter().rev() {
            if entry.bounds.contains_point(px, py) && entry.entity_id.is_valid() {
                return Some(entry.entity_id);
            }
        }
        None
    }

    /// Queries the front-most event listener registered for `event_type` at point (px, py).
    pub fn query_event_at_point(
        &self,
        px: f32,
        py: f32,
        event_type: EventType,
    ) -> Option<(EntityId, Rc<dyn Fn(Event)>, Rect)> {
        for entry in self.entries.iter().rev() {
            if entry.bounds.contains_point(px, py) {
                for (ev, handler) in &entry.event_listeners {
                    if *ev == event_type {
                        return Some((entry.entity_id, Rc::clone(handler), entry.bounds));
                    }
                }
            }
        }
        None
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
