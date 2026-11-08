//! De Casteljau's algorithm for cubic Bézier evaluation, derivative computation,
//! and numerical root finding for CSS transition and animation timing functions.

/// Evaluates a 1D cubic Bézier curve at parameter `u` in `[0.0, 1.0]` using De Casteljau's algorithm.
///
/// Control points are fixed at \(P_0 = 0.0\), \(P_1 = p_1\), \(P_2 = p_2\), \(P_3 = 1.0\).
#[inline(always)]
pub fn de_casteljau_eval_1d(p1: f32, p2: f32, u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    let inv_u = 1.0 - u;

    // Level 1: linear combinations of adjacent control points
    let q0 = u * p1;
    let q1 = inv_u * p1 + u * p2;
    let q2 = inv_u * p2 + u;

    // Level 2: linear combinations of Level 1 points
    let r0 = inv_u * q0 + u * q1;
    let r1 = inv_u * q1 + u * q2;

    // Level 3: final curve point
    inv_u * r0 + u * r1
}

/// Evaluates both the 1D cubic Bézier curve value and its first derivative \(B'(u)\) at parameter `u`.
///
/// Uses the De Casteljau intermediate subsegment points where \(B'(u) = 3(r_1 - r_0)\).
#[inline(always)]
pub fn de_casteljau_eval_and_derivative(p1: f32, p2: f32, u: f32) -> (f32, f32) {
    let u = u.clamp(0.0, 1.0);
    let inv_u = 1.0 - u;

    let q0 = u * p1;
    let q1 = inv_u * p1 + u * p2;
    let q2 = inv_u * p2 + u;

    let r0 = inv_u * q0 + u * q1;
    let r1 = inv_u * q1 + u * q2;

    let val = inv_u * r0 + u * r1;
    let deriv = 3.0 * (r1 - r0);

    (val, deriv)
}

/// Evaluates a 2D cubic Bézier point \((x(u), y(u))\) given control points \(P_1 = (x_1, y_1)\) and \(P_2 = (x_2, y_2)\).
#[inline(always)]
pub fn de_casteljau_eval_2d(x1: f32, y1: f32, x2: f32, y2: f32, u: f32) -> (f32, f32) {
    (
        de_casteljau_eval_1d(x1, x2, u),
        de_casteljau_eval_1d(y1, y2, u),
    )
}

/// Solves for progress \(y\) given time fraction \(x \in [0.0, 1.0]\) along the cubic Bézier curve
/// defined by \(P_1 = (x_1, y_1)\) and \(P_2 = (x_2, y_2)\).
///
/// Implements Newton-Raphson iteration accelerated by De Casteljau derivatives with an automatic
/// fallback to De Casteljau interval subdivision (bisection root finding) whenever the curve
/// exhibits near-zero derivatives, inflection points, or out-of-bounds steps.
pub fn cubic_bezier_solve(x: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }

    // Linear fast path: diagonal control points yield y = x identically
    if (x1 - y1).abs() < 1e-6 && (x2 - y2).abs() < 1e-6 {
        return x;
    }

    // Step 1: Attempt Newton-Raphson iteration with De Casteljau derivative
    let mut u = x;
    let mut converged = false;

    for _ in 0..8 {
        let (bx, dbx) = de_casteljau_eval_and_derivative(x1, x2, u);
        let err = bx - x;
        if err.abs() < 1e-6 {
            converged = true;
            break;
        }
        if dbx.abs() < 1e-6 {
            break;
        }
        let u_next = u - err / dbx;
        if !(0.0..=1.0).contains(&u_next) {
            break;
        }
        u = u_next;
    }

    // Step 2: Fallback to De Casteljau binary interval subdivision if Newton-Raphson stalled
    if !converged {
        let mut u_min = 0.0f32;
        let mut u_max = 1.0f32;
        u = x;

        for _ in 0..20 {
            let bx = de_casteljau_eval_1d(x1, x2, u);
            let err = bx - x;
            if err.abs() < 1e-6 {
                break;
            }
            if err > 0.0 {
                u_max = u;
            } else {
                u_min = u;
            }
            u = 0.5 * (u_min + u_max);
        }
    }

    // Step 3: Compute final vertical progression y(u)
    de_casteljau_eval_1d(y1, y2, u)
}
