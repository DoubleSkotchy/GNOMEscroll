use rayon::prelude::*;

pub struct FrameStitcher {
    width: usize,
    height: usize,
    canvas: Vec<u8>,
    pub current_height: usize,
    anchor_frame: Option<Vec<u8>>,
    max_delta: usize,
    pub stitched_count: usize,
}

impl FrameStitcher {
    pub fn new(width: u32, height: u32) -> Self {
        let w = width as usize;
        let h = height as usize;
        let max_delta = (h as f64 * 0.85) as usize;

        Self {
            width: w,
            height: h,
            canvas: Vec::new(),
            current_height: 0,
            anchor_frame: None,
            max_delta: max_delta.max(60),
            stitched_count: 0,
        }
    }

    /// Process a new incoming RGBA frame from screencast.
    ///
    /// Uses a 3-stage approach inspired by ShareX / Shottr:
    ///  1. Extract a ROI strip from the bottom of the anchor frame
    ///  2. Search for that strip in the top portion of the new frame (coarse → fine)
    ///  3. Validate the match with full-overlap verification + anti-superimposition gates
    pub fn feed_frame(&mut self, frame: &[u8]) -> bool {
        let expected_size = self.width * self.height * 4;
        if frame.len() != expected_size {
            return false;
        }

        // 1. First frame: initialize canvas and anchor
        if self.anchor_frame.is_none() {
            self.canvas.extend_from_slice(frame);
            self.current_height = self.height;
            self.anchor_frame = Some(frame.to_vec());
            self.stitched_count = 1;
            return true;
        }

        let anchor = self.anchor_frame.as_ref().unwrap();

        // Margins to exclude scrollbar (right) and window border (left)
        let left_m = 16.min(self.width / 12);
        let right_m = self.width.saturating_sub(32.min(self.width / 12));
        if right_m <= left_m + 50 {
            return false;
        }

        let max_d = self.max_delta.min(self.height.saturating_sub(25));
        if max_d < 1 {
            return false;
        }

        // ══════════════════════════════════════════════════════════════
        // STAGE 1: ROI Strip Matching (fast, robust)
        // ══════════════════════════════════════════════════════════════
        // Extract a strip of rows from the BOTTOM of the anchor.
        // Search for where this strip appears in the TOP of the new frame.
        // If anchor scrolled down by delta pixels, then:
        //   anchor[delta..H] should match frame[0..H-delta]
        // Equivalently: anchor_bottom_strip starting at row (H - strip_h)
        //   should appear in frame at row (H - strip_h - delta).
        //
        // We use a strip from the bottom 15% of the anchor (but at least 20 rows,
        // at most 80 rows) as the ROI template.

        let strip_h = ((self.height as f64 * 0.15) as usize).clamp(20, 80.min(self.height / 3));
        let strip_start_in_anchor = self.height - strip_h; // row index in anchor

        // For each candidate delta, the strip should appear in the new frame at:
        //   frame_row = strip_start_in_anchor - delta
        // which means delta can be at most strip_start_in_anchor (so frame_row >= 0).

        let effective_max_d = max_d.min(strip_start_in_anchor);

        // ── Coarse pass: row_step=2, col_step=4 ──
        let coarse_results: Vec<(usize, f64)> = (0..=effective_max_d)
            .into_par_iter()
            .map(|delta| {
                let sad = self.strip_sad(
                    anchor, frame, delta, strip_start_in_anchor, strip_h,
                    left_m, right_m, 2, 4,
                );
                (delta, sad)
            })
            .collect();

        if coarse_results.is_empty() {
            return false;
        }

        // Find top candidates
        let mut sorted = coarse_results;
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let coarse_best_delta = sorted[0].0;
        let coarse_best_sad = sorted[0].1;

        // Stationary detection: delta=0 is best and near-zero error
        if coarse_best_delta == 0 && coarse_best_sad < 1.5 {
            return false;
        }

        // If coarse best SAD is way too high, no match exists
        if coarse_best_sad > 12.0 {
            return false;
        }

        // ── Fine pass: row_step=1, col_step=1, around best ±3 ──
        let fine_start = coarse_best_delta.saturating_sub(3).max(1);
        let fine_end = (coarse_best_delta + 3).min(effective_max_d);

        let mut best_delta = coarse_best_delta;
        let mut best_sad = f64::MAX;

        for d in fine_start..=fine_end {
            let sad = self.strip_sad(
                anchor, frame, d, strip_start_in_anchor, strip_h,
                left_m, right_m, 1, 1,
            );
            if sad < best_sad {
                best_sad = sad;
                best_delta = d;
            }
        }

        if best_delta == 0 {
            return false;
        }

        // ══════════════════════════════════════════════════════════════
        // STAGE 2: Full-Overlap Verification
        // ══════════════════════════════════════════════════════════════
        // Verify the ENTIRE overlapping region, not just the strip.
        // anchor[best_delta .. H] should match frame[0 .. H - best_delta]

        let overlap_h = self.height - best_delta;
        let (full_sad, ink_sad, ink_count) =
            self.full_overlap_metrics(anchor, frame, best_delta, overlap_h, left_m, right_m);

        // ── Validation Gates ──
        // Gate A: Global overlap error — must be very low for a correct match
        if full_sad > 4.0 {
            return false;
        }

        // Gate B: Anti-superimposition — if there's significant ink (text/lines),
        // the ink-only SAD must also be low. High ink SAD means text from two
        // different positions is overlapping.
        if ink_count >= 20 && ink_sad > 12.0 {
            return false;
        }

        // Gate C: If barely any features, require extremely tight match
        if ink_count < 15 && full_sad > 0.6 {
            return false;
        }

        // Gate D: Uniqueness — reject if there's a close runner-up at a very
        // different delta (periodic text patterns can cause this)
        if sorted.len() > 1 {
            let best_coarse_sad_refined = best_sad; // fine-pass SAD for best delta
            for other in &sorted[1..5.min(sorted.len())] {
                let d_diff = (other.0 as isize - best_delta as isize).unsigned_abs();
                if d_diff > 6 && other.1 < coarse_best_sad * 1.35 && full_sad > 1.8 {
                    // Another delta far away scored nearly as well → ambiguous
                    return false;
                }
                // Also check: if the best coarse is barely better than many
                // runners-up, it's probably periodic content
                if d_diff > 3 && other.1 < best_coarse_sad_refined * 1.08 && full_sad > 1.0 {
                    return false;
                }
            }
        }

        // ══════════════════════════════════════════════════════════════
        // STAGE 3: Append new content
        // ══════════════════════════════════════════════════════════════
        let new_start_row = self.height - best_delta;
        let new_start_byte = new_start_row * self.width * 4;
        let new_end_byte = self.height * self.width * 4;

        self.canvas.extend_from_slice(&frame[new_start_byte..new_end_byte]);
        self.current_height += best_delta;
        self.anchor_frame = Some(frame.to_vec());
        self.stitched_count += 1;
        true
    }

    /// Compute SAD between a strip in the anchor and the corresponding position in the new frame.
    /// For delta d: anchor rows [strip_start .. strip_start + strip_h]
    ///              match against frame rows [strip_start - d .. strip_start - d + strip_h]
    #[inline]
    fn strip_sad(
        &self,
        anchor: &[u8],
        curr: &[u8],
        delta: usize,
        strip_start: usize,  // row in anchor where strip begins
        strip_h: usize,
        left_m: usize,
        right_m: usize,
        row_step: usize,
        col_step: usize,
    ) -> f64 {
        let w = self.width;
        // Frame row = strip_start - delta + r
        let frame_row_start = strip_start.checked_sub(delta);
        if frame_row_start.is_none() {
            return 255.0;
        }
        let frame_row_start = frame_row_start.unwrap();

        let mut total_diff = 0u64;
        let mut total_samples = 0u64;

        for r in (0..strip_h).step_by(row_step) {
            let a_row = strip_start + r;
            let f_row = frame_row_start + r;

            if a_row >= self.height || f_row >= self.height {
                break;
            }

            let a_off = a_row * w * 4;
            let f_off = f_row * w * 4;

            for c in (left_m..right_m).step_by(col_step) {
                let ai = a_off + c * 4;
                let fi = f_off + c * 4;

                let dr = (anchor[ai] as i32 - curr[fi] as i32).unsigned_abs() as u64;
                let dg = (anchor[ai + 1] as i32 - curr[fi + 1] as i32).unsigned_abs() as u64;
                let db = (anchor[ai + 2] as i32 - curr[fi + 2] as i32).unsigned_abs() as u64;

                total_diff += dr + dg + db;
                total_samples += 3;
            }
        }

        if total_samples > 0 {
            total_diff as f64 / total_samples as f64
        } else {
            255.0
        }
    }

    /// Full-overlap verification: compute global SAD + ink-only SAD across
    /// the entire overlapping rectangle.
    fn full_overlap_metrics(
        &self,
        anchor: &[u8],
        curr: &[u8],
        delta: usize,
        overlap_h: usize,
        left_m: usize,
        right_m: usize,
    ) -> (f64, f64, usize) {
        let w = self.width;
        // row_step=2, col_step=2 for speed (still evaluates ~25% of all pixels)
        let row_step = 2;
        let col_step = 2;

        let mut total_diff = 0u64;
        let mut total_samples = 0u64;
        let mut ink_diff = 0u64;
        let mut ink_samples = 0usize;

        for r in (0..overlap_h).step_by(row_step) {
            let a_off = (delta + r) * w * 4;
            let c_off = r * w * 4;

            for c in (left_m..right_m).step_by(col_step) {
                let ai = a_off + c * 4;
                let ci = c_off + c * 4;

                let ar = anchor[ai] as i32;
                let ag = anchor[ai + 1] as i32;
                let ab = anchor[ai + 2] as i32;

                let cr = curr[ci] as i32;
                let cg = curr[ci + 1] as i32;
                let cb = curr[ci + 2] as i32;

                let dr = (ar - cr).unsigned_abs() as u64;
                let dg = (ag - cg).unsigned_abs() as u64;
                let db = (ab - cb).unsigned_abs() as u64;

                let d_sum = dr + dg + db;
                total_diff += d_sum;
                total_samples += 3;

                // Ink detection: if either pixel is non-white (lum < 235)
                let a_lum = (ar + ag + ab) / 3;
                let c_lum = (cr + cg + cb) / 3;
                if a_lum < 235 || c_lum < 235 {
                    ink_diff += d_sum;
                    ink_samples += 3;
                }
            }
        }

        let total_sad = if total_samples > 0 {
            total_diff as f64 / total_samples as f64
        } else {
            255.0
        };

        let ink_sad = if ink_samples > 0 {
            ink_diff as f64 / ink_samples as f64
        } else {
            0.0
        };

        (total_sad, ink_sad, ink_samples)
    }

    /// Return the finalized RGBA image dimensions, total stitched frame count, and pixel buffer
    pub fn finish(self) -> (u32, u32, usize, Vec<u8>) {
        (self.width as u32, self.current_height as u32, self.stitched_count, self.canvas)
    }
}
