//! Largest Remainder Quantization (Hare-Niemeyer Algorithm) for Discrete Grids and Terminals.

/// Distributes a discrete cell width/height across floating-point shares using the
/// Largest Remainder Method (Hare-Niemeyer Algorithm).
///
/// Guarantees that the sum of returned cell tracks equals `total_available` exactly,
/// completely eliminating 1-cell gap errors and container overflow line-wraps.
pub fn largest_remainder_quantization(shares: &[f32], total_available: u16) -> Vec<u16> {
    let n = shares.len();
    if n == 0 {
        return Vec::new();
    }
    if total_available == 0 {
        return vec![0; n];
    }

    let sum_shares: f32 = shares.iter().filter(|&&s| s > 0.0).sum();
    if sum_shares <= 0.0 {
        // Evenly distribute if all shares are zero or negative
        let base = total_available / (n as u16);
        let mut rem = total_available % (n as u16);
        let mut out = vec![base; n];
        for val in out.iter_mut() {
            if rem > 0 {
                *val += 1;
                rem -= 1;
            }
        }
        return out;
    }

    let total_f = total_available as f32;
    let mut allocated = Vec::with_capacity(n);
    let mut remainders = Vec::with_capacity(n);
    let mut current_sum = 0u16;

    for (idx, &s) in shares.iter().enumerate() {
        let proportion = (s.max(0.0) / sum_shares) * total_f;
        let floor_val = proportion.floor() as u16;
        let remainder = proportion - (floor_val as f32);

        allocated.push(floor_val);
        remainders.push((remainder, idx));
        current_sum = current_sum.saturating_add(floor_val);
    }

    // Sort remainders descending by remainder value
    remainders.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let deficit = total_available.saturating_sub(current_sum);
    for &(_, track_idx) in &remainders[..(deficit as usize).min(n)] {
        allocated[track_idx] += 1;
    }

    allocated
}
