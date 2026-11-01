//! The 5 Axioms of Layout Containment and Hermetic Partition Boundary Classification.

use strata_dom::NodeId;
use strata_layout::Rect;
use strata_style::{ComputedStyle, Display, Overflow, Position};

/// Unique identifier for an independent layout partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PartitionId(pub u32);

/// An identified independent layout boundary node satisfying the 5 Axioms of Containment.
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionBoundary {
    pub node_id: NodeId,
    pub partition_id: PartitionId,
    pub bounds: Rect,
    pub z_index: i32,
}

/// Evaluator for the 5 Axioms of Layout Containment.
pub struct ContainmentClassifier;

impl ContainmentClassifier {
    /// Evaluates whether a node establishes an Independent Layout Boundary
    /// according to the 5 Axioms of Layout Containment.
    ///
    /// When a node qualifies, its subtree can be reconciled, styled, and laid out
    /// in parallel on background worker threads without mutating or querying parent geometry.
    pub fn is_layout_boundary(
        style: &ComputedStyle,
        width_definite: bool,
        height_definite: bool,
    ) -> bool {
        // Axiom 1: Sizing Definiteness & Content Invariance
        let axiom1 = width_definite && height_definite;

        // Axiom 2: Margin Collapsing Hermeticity
        let has_padding_or_border = style.padding.top > 0.0
            || style.padding.bottom > 0.0
            || style.border_width.top > 0.0
            || style.border_width.bottom > 0.0;
        let clips_overflow = style.overflow_x != Overflow::Visible || style.overflow_y != Overflow::Visible;
        let axiom2 = has_padding_or_border || clips_overflow;

        // Axiom 3: Spatial Reference & Coordinate Capture (Containing block for absolute descendants)
        let axiom3 = style.position != Position::Static || clips_overflow;

        // Axiom 4: Non-Feedback Parent Constraints (Does not alter parent sizing)
        let axiom4 = style.display != Display::Inline;

        // Axiom 5: Stacking Context & Clipping Hermeticity
        let axiom5 = clips_overflow || style.opacity < 1.0 || style.z_index.is_some();

        axiom1 && axiom2 && axiom3 && axiom4 && axiom5
    }
}
