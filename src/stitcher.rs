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

    /// Process a new incoming RGBA frame from screencast
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

        // Safe margins to exclude moving scrollbar on right and window border on left
        let left_m = 16.min(self.width / 12);
        let right_m = self.width.saturating_sub(32.min(self.width / 12));
        if right_m <= left_m + 50 {
            return false;
        }

        // 2. Parallel Coarse Scan across candidates delta \in [0, max_delta]
        // Including delta=0 naturally handles stationary / idle screen without heuristics!
        let max_d = self.max_delta.min(self.height.saturating_sub(25));
        if max_d < 1 {
            return false;
        }

        let coarse_candidates: Vec<(usize, f64)> = (0..=max_d)
            .into_par_iter()
            .map(|delta| {
                let sad = self.compute_fast_coarse_sad(anchor, frame, delta, left_m, right_m);
                (delta, sad)
            })
            .collect();

        if coarse_candidates.is_empty() {
            return false;
        }

        let mut sorted_coarse = coarse_candidates;
        sorted_coarse.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let best_coarse_delta = sorted_coarse[0].0;

        // If delta=0 is the best match with near-zero SAD, viewport is stationary
        if best_coarse_delta == 0 && sorted_coarse[0].1 < 1.5 {
            return false;
        }

        // 3. Fine Refinement around top candidate (delta +/- 2)
        let fine_start = best_coarse_delta.saturating_sub(2).max(1);
        let fine_end = (best_coarse_delta + 2).min(max_d);

        let mut best_delta = best_coarse_delta;
        let mut min_sad = f64::MAX;
        let mut best_ink_sad = f64::MAX;
        let mut best_ink_count = 0usize;

        for d in fine_start..=fine_end {
            let (s, is, ic) = self.compute_fine_metrics(anchor, frame, d, left_m, right_m);
            if s < min_sad {
                min_sad = s;
                best_ink_sad = is;
                best_ink_count = ic;
                best_delta = d;
            }
        }

        if best_delta == 0 {
            return false;
        }

        // 4. Mathematical Anti-Superimposition & Validation Gates
        // Gate A: Global overlap error threshold
        if min_sad > 4.8 {
            return false;
        }

        // Gate B: Text/Ink error threshold (completely prevents colliding lines like 8.2 on 8.3)
        if best_ink_count >= 20 && best_ink_sad > 14.0 {
            return false;
        }

        // Gate C: Minimum feature presence
        if best_ink_count < 15 && min_sad > 0.8 {
            return false;
        }

        // Gate D: Uniqueness check against repetitive patterns
        if sorted_coarse.len() > 1 {
            for other in &sorted_coarse[1..] {
                let diff_d = (other.0 as isize - best_delta as isize).abs();
                if diff_d > 6 {
                    if other.1 < min_sad * 1.30 && min_sad > 2.0 {
                        return false;
                    }
                    break;
                }
            }
        }

        // Verified High-Confidence Match!
        // Append newly revealed bottom rows [height - best_delta .. height] from frame
        let start_row = self.height - best_delta;
        let start_byte = start_row * self.width * 4;
        let end_byte = self.height * self.width * 4;

        self.canvas.extend_from_slice(&frame[start_byte..end_byte]);
        self.current_height += best_delta;
        self.anchor_frame = Some(frame.to_vec());
        self.stitched_count += 1;
        true
    }

    /// Fast coarse overlap SAD using strided sampling (row_step=2, col_step=4)
    fn compute_fast_coarse_sad(
        &self,
        anchor: &[u8],
        curr: &[u8],
        delta: usize,
        left_m: usize,
        right_m: usize,
    ) -> f64 {
        let w = self.width;
        let overlap_h = self.height - delta;
        let row_step = 2;
        let col_step = 4;

        let mut total_diff = 0u64;
        let mut total_samples = 0u64;

        for r in (0..overlap_h).step_by(row_step) {
            let a_off = (delta + r) * w * 4;
            let c_off = r * w * 4;

            for c in (left_m..right_m).step_by(col_step) {
                let ai = a_off + c * 4;
                let ci = c_off + c * 4;

                let dr = (anchor[ai] as i32 - curr[ci] as i32).abs() as u64;
                let dg = (anchor[ai + 1] as i32 - curr[ci + 1] as i32).abs() as u64;
                let db = (anchor[ai + 2] as i32 - curr[ci + 2] as i32).abs() as u64;

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

    /// Stage 2: Fine full-resolution evaluation (row_step = 1) for exact integer precision
    fn compute_fine_metrics(
        &self,
        anchor: &[u8],
        curr: &[u8],
        delta: usize,
        left_m: usize,
        right_m: usize,
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

            for c in (left_m..right_m).step_by(col_step) {
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
