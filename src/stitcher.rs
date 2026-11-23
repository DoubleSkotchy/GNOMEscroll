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
        // Search up to 85% of the frame height per step to accommodate large mouse wheel jumps
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

    /// Process a new incoming RGBA frame from screencast
    pub fn feed_frame(&mut self, frame: &[u8]) -> bool {
        let expected_size = self.width * self.height * 4;
        if frame.len() != expected_size {
            return false;
        }

        // First frame: initialize canvas and anchor
        if self.anchor_frame.is_none() {
            self.canvas.extend_from_slice(frame);
            self.current_height = self.height;
            self.anchor_frame = Some(frame.to_vec());
            self.stitched_count = 1;
            return true;
        }

        let anchor = self.anchor_frame.as_ref().unwrap();

        // 1. Fast stationary check: if screen didn't scroll, skip
        if self.is_stationary(anchor, frame) {
            return false;
        }

        // 2. Full-Overlap Parallel Alignment Search
        // We evaluate candidate downward shifts delta \in [1, max_delta].
        // For a shift of delta:
        // anchor rows [delta .. height] must match curr rows [0 .. height - delta]
        let max_d = self.max_delta.min(self.height.saturating_sub(30));
        if max_d < 1 {
            return false;
        }

        let candidates: Vec<(usize, f64, f64, usize)> = (1..=max_d)
            .into_par_iter()
            .map(|delta| {
                let (sad, ink_sad, ink_count) = self.compute_overlap_metrics(anchor, frame, delta);
                (delta, sad, ink_sad, ink_count)
            })
            .collect();

        if candidates.is_empty() {
            return false;
        }

        // Find the best candidates from coarse pass
        let mut sorted = candidates.clone();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let coarse_best_delta = sorted[0].0;

        // Stage 2: Fine full-resolution refinement around coarse best (delta +/- 3)
        // With row_step = 1, every row is evaluated, eliminating any odd/even sampling parity bias.
        let fine_start = coarse_best_delta.saturating_sub(3).max(1);
        let fine_end = (coarse_best_delta + 3).min(max_d);

        let mut best_delta = coarse_best_delta;
        let mut min_sad = f64::MAX;
        let mut ink_sad = f64::MAX;
        let mut ink_count = 0usize;

        for d in fine_start..=fine_end {
            let (s, is, ic) = self.compute_fine_overlap_metrics(anchor, frame, d);
            if s < min_sad {
                min_sad = s;
                ink_sad = is;
                ink_count = ic;
                best_delta = d;
            }
        }

        // 3. Mathematical Verification & Anti-Superimposition Gates
        // Gate A: Global overlap SAD must be clean (typical match < 3.0, allow up to 4.8 for slight anti-aliasing)
        if min_sad > 4.8 {
            return false;
        }

        // Gate B: Text/Ink SAD check (strictly prevents line-to-line collisions like 8.2 on 8.3)
        // When text lines overlap incorrectly, ink_sad is > 40.0. True alignment has ink_sad < 8.0.
        if ink_count >= 30 && ink_sad > 14.0 {
            return false;
        }

        // Gate C: Minimum feature presence
        // If the entire overlapping region is pure blank paper (virtually zero ink),
        // we do not guess an arbitrary shift; wait until content enters the frame.
        if ink_count < 20 && min_sad > 0.8 {
            return false;
        }

        // Gate D: Uniqueness / Ambiguity ratio
        // If there's another local minimum outside delta +/- 6 with nearly identical SAD,
        // it indicates repetitive periodic patterns (like zebra stripes or dense identical grids).
        if sorted.len() > 1 {
            for other in &sorted[1..] {
                let diff_d = (other.0 as isize - best_delta as isize).abs();
                if diff_d > 6 {
                    // If competitor is too close in SAD and min_sad isn't near zero, reject ambiguous match
                    if other.1 < min_sad * 1.30 && min_sad > 2.0 {
                        return false;
                    }
                    break;
                }
            }
        }

        // Verified High-Confidence Match!
        // Append rows [height - best_delta .. height] from current frame
        let start_row = self.height - best_delta;
        let start_byte = start_row * self.width * 4;
        let end_byte = self.height * self.width * 4;

        self.canvas.extend_from_slice(&frame[start_byte..end_byte]);
        self.current_height += best_delta;
        // Update anchor frame to the newly stitched frame
        self.anchor_frame = Some(frame.to_vec());
        self.stitched_count += 1;
        true
    }

    /// Fast check if consecutive frames are essentially identical (cursor blink, idle)
    fn is_stationary(&self, anchor: &[u8], curr: &[u8]) -> bool {
        let w = self.width;
        let h = self.height;
        let row_step = 4;
        let col_step = 8;
        let mut diff_sum = 0u64;
        let mut max_diff = 0i32;
        let mut count = 0u64;

        for r in (0..h).step_by(row_step) {
            let row_off = r * w * 4;
            for c in (0..w).step_by(col_step) {
                let idx = row_off + c * 4;
                let dr = (anchor[idx] as i32 - curr[idx] as i32).abs();
                let dg = (anchor[idx + 1] as i32 - curr[idx + 1] as i32).abs();
                let db = (anchor[idx + 2] as i32 - curr[idx + 2] as i32).abs();
                let m = dr.max(dg).max(db);
                if m > max_diff {
                    max_diff = m;
                }
                diff_sum += (dr + dg + db) as u64;
                count += 3;
            }
        }

        let avg = if count > 0 { diff_sum as f64 / count as f64 } else { 0.0 };
        // Stationary only if whole-frame average is negligible AND no significant pixel changed
        avg < 0.4 && max_diff < 20
    }

    /// Compute full-overlap SAD and ink-weighted SAD for a given downward delta shift (coarse pass)
    fn compute_overlap_metrics(
        &self,
        anchor: &[u8],
        curr: &[u8],
        delta: usize,
    ) -> (f64, f64, usize) {
        let w = self.width;
        let overlap_h = self.height - delta;
        let row_step = 2;
        let col_step = 4;

        let mut total_diff = 0u64;
        let mut total_samples = 0u64;

        let mut ink_diff = 0u64;
        let mut ink_samples = 0usize;

        for r in (0..overlap_h).step_by(row_step) {
            let a_off = (delta + r) * w * 4;
            let c_off = r * w * 4;

            for c in (0..w).step_by(col_step) {
                let ai = a_off + c * 4;
                let ci = c_off + c * 4;

                let ar = anchor[ai] as i32;
                let ag = anchor[ai + 1] as i32;
                let ab = anchor[ai + 2] as i32;

                let cr = curr[ci] as i32;
                let cg = curr[ci + 1] as i32;
                let cb = curr[ci + 2] as i32;

                let dr = (ar - cr).abs() as u64;
                let dg = (ag - cg).abs() as u64;
                let db = (ab - cb).abs() as u64;

                let d_sum = dr + dg + db;
                total_diff += d_sum;
                total_samples += 3;

                // Ink/Text detector: pixels that are not pure background (brightness < 235)
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

    /// Stage 2: Fine full-resolution evaluation (row_step = 1) for sub-pixel/exact integer precision
    fn compute_fine_overlap_metrics(
        &self,
        anchor: &[u8],
        curr: &[u8],
        delta: usize,
    ) -> (f64, f64, usize) {
        let w = self.width;
        let overlap_h = self.height - delta;
        let row_step = 1; // Evaluates EVERY row!
        let col_step = 2;

        let mut total_diff = 0u64;
        let mut total_samples = 0u64;

        let mut ink_diff = 0u64;
        let mut ink_samples = 0usize;

        for r in (0..overlap_h).step_by(row_step) {
            let a_off = (delta + r) * w * 4;
            let c_off = r * w * 4;

            for c in (0..w).step_by(col_step) {
                let ai = a_off + c * 4;
                let ci = c_off + c * 4;

                let ar = anchor[ai] as i32;
                let ag = anchor[ai + 1] as i32;
                let ab = anchor[ai + 2] as i32;

                let cr = curr[ci] as i32;
                let cg = curr[ci + 1] as i32;
                let cb = curr[ci + 2] as i32;

                let dr = (ar - cr).abs() as u64;
                let dg = (ag - cg).abs() as u64;
                let db = (ab - cb).abs() as u64;

                let d_sum = dr + dg + db;
                total_diff += d_sum;
                total_samples += 3;

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
