//! Floating UI & Collision Solver for Tooltips, Popovers, Dropdowns, and Menus.

/// 2D bounding rectangle for layout anchoring and collision resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl FloatRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}

pub type Rect = FloatRect;

/// Placement orientation for floating elements relative to anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Top,
    TopStart,
    TopEnd,
    Bottom,
    BottomStart,
    BottomEnd,
    Left,
    LeftStart,
    LeftEnd,
    Right,
    RightStart,
    RightEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Start,
    Center,
    End,
}

impl Placement {
    pub fn side(&self) -> Side {
        match self {
            Placement::Top | Placement::TopStart | Placement::TopEnd => Side::Top,
            Placement::Bottom | Placement::BottomStart | Placement::BottomEnd => Side::Bottom,
            Placement::Left | Placement::LeftStart | Placement::LeftEnd => Side::Left,
            Placement::Right | Placement::RightStart | Placement::RightEnd => Side::Right,
        }
    }

    pub fn alignment(&self) -> Alignment {
        match self {
            Placement::TopStart | Placement::BottomStart | Placement::LeftStart | Placement::RightStart => Alignment::Start,
            Placement::TopEnd | Placement::BottomEnd | Placement::LeftEnd | Placement::RightEnd => Alignment::End,
            _ => Alignment::Center,
        }
    }

    pub fn opposite(&self) -> Self {
        match self {
            Placement::Top => Placement::Bottom,
            Placement::TopStart => Placement::BottomStart,
            Placement::TopEnd => Placement::BottomEnd,
            Placement::Bottom => Placement::Top,
            Placement::BottomStart => Placement::TopStart,
            Placement::BottomEnd => Placement::TopEnd,
            Placement::Left => Placement::Right,
            Placement::LeftStart => Placement::RightStart,
            Placement::LeftEnd => Placement::RightEnd,
            Placement::Right => Placement::Left,
            Placement::RightStart => Placement::LeftStart,
            Placement::RightEnd => Placement::LeftEnd,
        }
    }
}

/// Configuration options for floating positioning calculation.
#[derive(Debug, Clone, Copy)]
pub struct ComputePositionOptions {
    pub placement: Placement,
    pub offset: f32,
    pub flip: bool,
    pub shift: bool,
    pub viewport: FloatRect,
}

impl Default for ComputePositionOptions {
    fn default() -> Self {
        Self {
            placement: Placement::Bottom,
            offset: 8.0,
            flip: true,
            shift: true,
            viewport: FloatRect::new(0.0, 0.0, 1920.0, 1080.0),
        }
    }
}

/// Resulting coordinates and resolved placement after collision handling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComputePositionResult {
    pub x: f32,
    pub y: f32,
    pub actual_placement: Placement,
}

/// Solves 2D floating placement relative to an anchor rectangle with flip and shift boundary constraints.
pub fn compute_position(
    anchor: FloatRect,
    floating: FloatRect,
    options: ComputePositionOptions,
) -> ComputePositionResult {
    let mut placement = options.placement;

    // Helper to calculate raw coords for a given placement
    let calculate_coords = |p: Placement| -> (f32, f32) {
        let (mut x, mut y) = match p.side() {
            Side::Top => (0.0, anchor.y - floating.height - options.offset),
            Side::Bottom => (0.0, anchor.y + anchor.height + options.offset),
            Side::Left => (anchor.x - floating.width - options.offset, 0.0),
            Side::Right => (anchor.x + anchor.width + options.offset, 0.0),
        };

        match p.side() {
            Side::Top | Side::Bottom => {
                x = match p.alignment() {
                    Alignment::Start => anchor.x,
                    Alignment::Center => anchor.x + (anchor.width - floating.width) / 2.0,
                    Alignment::End => anchor.x + anchor.width - floating.width,
                };
            }
            Side::Left | Side::Right => {
                y = match p.alignment() {
                    Alignment::Start => anchor.y,
                    Alignment::Center => anchor.y + (anchor.height - floating.height) / 2.0,
                    Alignment::End => anchor.y + anchor.height - floating.height,
                };
            }
        }

        (x, y)
    };

    let (mut x, mut y) = calculate_coords(placement);

    // Flip collision detection
    if options.flip {
        let overflows = match placement.side() {
            Side::Top => y < options.viewport.y,
            Side::Bottom => y + floating.height > options.viewport.y + options.viewport.height,
            Side::Left => x < options.viewport.x,
            Side::Right => x + floating.width > options.viewport.x + options.viewport.width,
        };

        if overflows {
            let flipped = placement.opposite();
            let (fx, fy) = calculate_coords(flipped);
            let flipped_overflows = match flipped.side() {
                Side::Top => fy < options.viewport.y,
                Side::Bottom => fy + floating.height > options.viewport.y + options.viewport.height,
                Side::Left => fx < options.viewport.x,
                Side::Right => fx + floating.width > options.viewport.x + options.viewport.width,
            };

            if !flipped_overflows {
                placement = flipped;
                x = fx;
                y = fy;
            }
        }
    }

    // Shift collision detection (constrain into viewport)
    if options.shift {
        let min_x = options.viewport.x;
        let max_x = options.viewport.x + options.viewport.width - floating.width;
        let min_y = options.viewport.y;
        let max_y = options.viewport.y + options.viewport.height - floating.height;

        if max_x >= min_x {
            x = x.clamp(min_x, max_x);
        }
        if max_y >= min_y {
            y = y.clamp(min_y, max_y);
        }
    }

    ComputePositionResult {
        x,
        y,
        actual_placement: placement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_position_bottom_centered() {
        let anchor = Rect::new(100.0, 100.0, 80.0, 40.0);
        let floating = Rect::new(0.0, 0.0, 100.0, 50.0);
        let options = ComputePositionOptions {
            placement: Placement::Bottom,
            offset: 10.0,
            flip: false,
            shift: false,
            viewport: Rect::new(0.0, 0.0, 1000.0, 1000.0),
        };

        let res = compute_position(anchor, floating, options);
        assert_eq!(res.y, 150.0); // 100 + 40 + 10
        assert_eq!(res.x, 90.0);  // 100 + (80 - 100) / 2 = 90
        assert_eq!(res.actual_placement, Placement::Bottom);
    }

    #[test]
    fn test_compute_position_flip_on_boundary() {
        // Anchor near bottom of viewport
        let anchor = Rect::new(100.0, 950.0, 80.0, 40.0);
        let floating = Rect::new(0.0, 0.0, 100.0, 50.0);
        let options = ComputePositionOptions {
            placement: Placement::Bottom,
            offset: 10.0,
            flip: true,
            shift: true,
            viewport: Rect::new(0.0, 0.0, 1000.0, 1000.0),
        };

        let res = compute_position(anchor, floating, options);
        // Bottom would be 950 + 40 + 10 = 1000, height 50 -> 1050 > 1000. Flips to Top!
        assert_eq!(res.actual_placement, Placement::Top);
        assert_eq!(res.y, 890.0); // 950 - 50 - 10 = 890
    }
}
