//! Analytical signed distance fields (SDF) and winding numbers for quadratic Béziers.

use crate::curve::{Point2D, QuadraticBezier};

/// Computes the exact minimum Euclidean distance from query point `p` to quadratic Bézier `curve`.
pub fn distance_to_quadratic(p: Point2D, curve: &QuadraticBezier) -> f32 {
    // Express curve in polynomial form: B(t) = a * t^2 + b * t + c
    // B(t) = (1-t)^2 P0 + 2t(1-t) P1 + t^2 P2
    //      = (P0 - 2P1 + P2) t^2 + 2(P1 - P0) t + P0
    let a_x = curve.p0.x - 2.0 * curve.p1.x + curve.p2.x;
    let a_y = curve.p0.y - 2.0 * curve.p1.y + curve.p2.y;

    let b_x = 2.0 * (curve.p1.x - curve.p0.x);
    let b_y = 2.0 * (curve.p1.y - curve.p0.y);

    let c_x = curve.p0.x - p.x;
    let c_y = curve.p0.y - p.y;

    // We seek t minimizing || B(t) - P ||^2
    // d/dt || B(t) - P ||^2 = 2 * (B(t) - P) . B'(t) = 0
    // (a t^2 + b t + c) . (2 a t + b) = 0
    // 2 (a.a) t^3 + 3 (a.b) t^2 + (2 a.c + b.b) t + (b.c) = 0
    let dot_aa = a_x * a_x + a_y * a_y;
    let dot_ab = a_x * b_x + a_y * b_y;
    let dot_ac = a_x * c_x + a_y * c_y;
    let dot_bb = b_x * b_x + b_y * b_y;
    let dot_bc = b_x * c_x + b_y * c_y;

    let k3 = 2.0 * dot_aa;
    let k2 = 3.0 * dot_ab;
    let k1 = 2.0 * dot_ac + dot_bb;
    let k0 = dot_bc;

    let mut min_dist_sq = p.distance_squared(curve.p0).min(p.distance_squared(curve.p2));

    let roots = solve_cubic_roots(k3, k2, k1, k0);
    for t in roots {
        if (0.0..=1.0).contains(&t) {
            let pt_on_curve = curve.evaluate(t);
            let d2 = p.distance_squared(pt_on_curve);
            if d2 < min_dist_sq {
                min_dist_sq = d2;
            }
        }
    }

    min_dist_sq.sqrt()
}

/// Solves real roots of cubic equation: a * t^3 + b * t^2 + c * t + d = 0.
pub fn solve_cubic_roots(a: f32, b: f32, c: f32, d: f32) -> Vec<f32> {
    let mut roots = Vec::new();

    if a.abs() < 1e-7 {
        // Quadratic: b * t^2 + c * t + d = 0
        if b.abs() < 1e-7 {
            // Linear: c * t + d = 0
            if c.abs() > 1e-7 {
                roots.push(-d / c);
            }
        } else {
            let disc = c * c - 4.0 * b * d;
            if disc >= 0.0 {
                let sqrt_disc = disc.sqrt();
                roots.push((-c + sqrt_disc) / (2.0 * b));
                roots.push((-c - sqrt_disc) / (2.0 * b));
            }
        }
        return roots;
    }

    // Depressed cubic substitution: t = x - b / (3a)
    let inv_a = 1.0 / a;
    let p = (3.0 * a * c - b * b) / (3.0 * a * a);
    let q = (2.0 * b * b * b - 9.0 * a * b * c + 27.0 * a * a * d) / (27.0 * a * a * a);

    let offset = b * inv_a / 3.0;
    let disc = (q * q / 4.0) + (p * p * p / 27.0);

    if disc > 1e-7 {
        // One real root
        let sqrt_disc = disc.sqrt();
        let u = (-q / 2.0 + sqrt_disc).cbrt();
        let v = (-q / 2.0 - sqrt_disc).cbrt();
        roots.push(u + v - offset);
    } else if disc < -1e-7 {
        // Three real roots (Casus irreducibilis)
        let r = (-p * p * p / 27.0).sqrt();
        let phi = (-q / (2.0 * r)).clamp(-1.0, 1.0).acos();
        let cbrt_r = 2.0 * (-p / 3.0).sqrt();

        roots.push(cbrt_r * (phi / 3.0).cos() - offset);
        roots.push(cbrt_r * ((phi + 2.0 * std::f32::consts::PI) / 3.0).cos() - offset);
        roots.push(cbrt_r * ((phi + 4.0 * std::f32::consts::PI) / 3.0).cos() - offset);
    } else {
        // Multiple real roots
        let u = (-q / 2.0).cbrt();
        roots.push(2.0 * u - offset);
        roots.push(-u - offset);
    }

    roots
}

/// Evaluates whether point `p` is inside a closed path composed of quadratic Béziers using winding number.
pub fn is_point_inside_closed_path(p: Point2D, curves: &[QuadraticBezier]) -> bool {
    let mut winding = 0;

    for curve in curves {
        // Count crossings of a horizontal ray from p towards +X
        let p0 = curve.p0;
        let p2 = curve.p2;

        let crosses_y = (p0.y <= p.y && p2.y > p.y) || (p2.y <= p.y && p0.y > p.y);
        if crosses_y {
            // Solve curve y(t) = p.y
            // a_y t^2 + b_y t + c_y = 0
            let a_y = p0.y - 2.0 * curve.p1.y + p2.y;
            let b_y = 2.0 * (curve.p1.y - p0.y);
            let c_y = p0.y - p.y;

            let mut t_candidates = Vec::new();
            if a_y.abs() < 1e-6 {
                if b_y.abs() > 1e-6 {
                    t_candidates.push(-c_y / b_y);
                }
            } else {
                let disc = b_y * b_y - 4.0 * a_y * c_y;
                if disc >= 0.0 {
                    let s = disc.sqrt();
                    t_candidates.push((-b_y + s) / (2.0 * a_y));
                    t_candidates.push((-b_y - s) / (2.0 * a_y));
                }
            }

            for t in t_candidates {
                if (0.0..=1.0).contains(&t) {
                    let x_intersect = curve.evaluate(t).x;
                    if x_intersect >= p.x {
                        if p2.y > p0.y {
                            winding += 1;
                        } else {
                            winding -= 1;
                        }
                    }
                }
            }
        }
    }

    winding != 0
}
