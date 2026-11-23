#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrollDirection {
    Down,
    Up,
}

pub struct FrameStitcher {
    width: usize,
    height: usize,
    canvas: Vec<u8>,
    pub current_height: usize,
    anchor_frame: Option<Vec<u8>>,
    anchor_profiles: Option<(Vec<f32>, Vec<f32>, Vec<f32>)>,
    max_delta: usize,
    pub stitched_count: usize,
}

impl FrameStitcher {
    pub fn new(width: u32, height: u32) -> Self {
        let w = width as usize;
        let h = height as usize;
        let max_delta = (h as f64 * 0.90) as usize;

        Self {
            width: w,
            height: h,
            canvas: Vec::new(),
            current_height: 0,
            anchor_frame: None,
            anchor_profiles: None,
            max_delta: max_delta.max(60),
            stitched_count: 0,
        }
    }

    /// Process a new incoming RGBA frame from screencast.
    /// Supports bidirectional scrolling:
    /// - Scrolled DOWN: concats newly revealed rows downwards.
    /// - Scrolled UP: concats newly revealed rows upwards.
    pub fn feed_frame(&mut self, frame: &[u8]) -> bool {
        let expected_size = self.width * self.height * 4;
        if frame.len() != expected_size {
            return false;
        }

        // Margins to exclude scrollbar (right) and window borders (left)
        let left_m = 12.min(self.width / 16);
        let right_m = self.width.saturating_sub(32.min(self.width / 12));
        if right_m <= left_m + 30 {
            return false;
        }

        // Compute 3-strip 1D vertical luminance profile for the incoming frame
        let curr_profiles = self.compute_profiles(frame, left_m, right_m);

        // 1. First frame: initialize canvas and anchor
        if self.anchor_frame.is_none() {
            self.canvas.extend_from_slice(frame);
            self.current_height = self.height;
            self.anchor_frame = Some(frame.to_vec());
            self.anchor_profiles = Some(curr_profiles);
            self.stitched_count = 1;
            return true;
        }

        let anchor = self.anchor_frame.as_ref().unwrap();
        let (a_prof_l, a_prof_c, a_prof_r) = self.anchor_profiles.as_ref().unwrap();
        let (c_prof_l, c_prof_c, c_prof_r) = &curr_profiles;

        let max_d = self.max_delta.min(self.height.saturating_sub(25));
        if max_d < 2 {
            return false;
        }

        // ══════════════════════════════════════════════════════════════
        // STAGE 1: Bidirectional 1D Profile Scan for delta \in [0, max_d]
        // ══════════════════════════════════════════════════════════════
        // Evaluate both Scroll Down and Scroll Up candidates in 1D (~0.04ms)
        let mut candidates: Vec<(usize, ScrollDirection, f32)> = Vec::with_capacity(max_d * 2 + 1);

        // Delta = 0 (stationary)
        let mut sum_diff_0 = 0.0f32;
        for r in 0..self.height {
            sum_diff_0 += (a_prof_l[r] - c_prof_l[r]).abs()
                + (a_prof_c[r] - c_prof_c[r]).abs()
                + (a_prof_r[r] - c_prof_r[r]).abs();
        }
        let err_0 = sum_diff_0 / self.height as f32;
        candidates.push((0, ScrollDirection::Down, err_0));

        // Scan delta in 2..=max_d in both directions
        for delta in 2..=max_d {
            let overlap = self.height - delta;
            let mut sum_down = 0.0f32;
            let mut sum_up = 0.0f32;

            for r in 0..overlap {
                // Down: anchor scrolled down, content moved UP: anchor[delta + r] == curr[r]
                let dl_d = (a_prof_l[delta + r] - c_prof_l[r]).abs();
                let dc_d = (a_prof_c[delta + r] - c_prof_c[r]).abs();
                let dr_d = (a_prof_r[delta + r] - c_prof_r[r]).abs();
                sum_down += dl_d + dc_d + dr_d;

                // Up: anchor scrolled up, content moved DOWN: anchor[r] == curr[delta + r]
                let dl_u = (a_prof_l[r] - c_prof_l[delta + r]).abs();
                let dc_u = (a_prof_c[r] - c_prof_c[delta + r]).abs();
                let dr_u = (a_prof_r[r] - c_prof_r[delta + r]).abs();
                sum_up += dl_u + dc_u + dr_u;
            }

            let avg_down = sum_down / overlap as f32;
            let avg_up = sum_up / overlap as f32;

            candidates.push((delta, ScrollDirection::Down, avg_down));
            candidates.push((delta, ScrollDirection::Up, avg_up));
        }

        candidates.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        // ══════════════════════════════════════════════════════════════
        // STAGE 2: 2D Full-Overlap SAD on Top Candidates (~0.25ms)
        // ══════════════════════════════════════════════════════════════
        let top_k = 12.min(candidates.len());
        let mut tested = std::collections::HashSet::new();
        let mut to_test = Vec::new();

        // Always test delta=0
        to_test.push((0, ScrollDirection::Down));
        tested.insert((0, ScrollDirection::Down));

        for &(d, dir, _) in &candidates[..top_k] {
            if d == 0 {
                continue;
            }
            for neighbor in [d.saturating_sub(1), d, (d + 1).min(max_d)] {
                if neighbor >= 2 && tested.insert((neighbor, dir)) {
                    to_test.push((neighbor, dir));
                }
            }
        }

        let mut best_delta = 0usize;
        let mut best_dir = ScrollDirection::Down;
        let mut best_sad = f64::MAX;
        let mut best_ink_sad = f64::MAX;
        let mut best_ink_count = 0usize;

        // row_step = 1 ensures exact pixel alignment without parity bias
        for &(d, dir) in &to_test {
            let (sad, ink_sad, ink_count) =
                self.compute_overlap_metrics(anchor, frame, d, dir, left_m, right_m, 1, 4);

            if sad < best_sad {
                best_sad = sad;
                best_ink_sad = ink_sad;
                best_ink_count = ink_count;
                best_delta = d;
                best_dir = dir;
            }
        }

        // If delta=0 won or delta is too small, viewport is stationary
        if best_delta < 2 {
            return false;
        }

        // ══════════════════════════════════════════════════════════════
        // STAGE 3: Robust Validation Gates
        // ══════════════════════════════════════════════════════════════
        // Gate A: Global overlap difference threshold
        if best_sad > 8.0 {
            return false;
        }

        // Gate B: Text/Ink anti-superimposition gate
        if best_ink_count >= 25 && best_ink_sad > 24.0 {
            return false;
        }

        // Gate C: Minimum feature verification
        if best_ink_count < 10 && best_sad > 1.8 {
            return false;
        }

        // ══════════════════════════════════════════════════════════════
        // STAGE 4: Confirmed Match — Concat in Scroll Direction
        // ══════════════════════════════════════════════════════════════
        match best_dir {
            ScrollDirection::Down => {
                // Scrolled down: newly revealed rows at bottom of curr [H - best_delta .. H]
                let start_row = self.height - best_delta;
                let start_byte = start_row * self.width * 4;
                let end_byte = self.height * self.width * 4;
                self.canvas.extend_from_slice(&frame[start_byte..end_byte]);
            }
            ScrollDirection::Up => {
                // Scrolled up: newly revealed rows at top of curr [0 .. best_delta]
                let end_byte = best_delta * self.width * 4;
                self.canvas.splice(0..0, frame[..end_byte].iter().cloned());
            }
        }

        self.current_height += best_delta;
        self.anchor_frame = Some(frame.to_vec());
        self.anchor_profiles = Some(curr_profiles);
        self.stitched_count += 1;
        true
    }

    /// Compute 3 vertical strips (Left, Center, Right) 1D row luminance profiles
    fn compute_profiles(
        &self,
        frame: &[u8],
        left_m: usize,
        right_m: usize,
    ) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        let w = self.width;
        let h = self.height;

        let total_w = right_m - left_m;
        let strip_w = total_w / 3;

        let l_start = left_m;
        let l_end = left_m + strip_w;
        let c_start = l_end;
        let c_end = c_start + strip_w;
        let r_start = c_end;
        let r_end = right_m;

        let mut prof_l = Vec::with_capacity(h);
        let mut prof_c = Vec::with_capacity(h);
        let mut prof_r = Vec::with_capacity(h);

        for row in 0..h {
            let row_off = row * w * 4;

            let mut sum_l = 0u32;
            let mut count_l = 0u32;
            for col in (l_start..l_end).step_by(4) {
                let idx = row_off + col * 4;
                sum_l += frame[idx] as u32 + frame[idx + 1] as u32 + frame[idx + 2] as u32;
                count_l += 3;
            }

            let mut sum_c = 0u32;
            let mut count_c = 0u32;
            for col in (c_start..c_end).step_by(4) {
                let idx = row_off + col * 4;
                sum_c += frame[idx] as u32 + frame[idx + 1] as u32 + frame[idx + 2] as u32;
                count_c += 3;
            }

            let mut sum_r = 0u32;
            let mut count_r = 0u32;
            for col in (r_start..r_end).step_by(4) {
                let idx = row_off + col * 4;
                sum_r += frame[idx] as u32 + frame[idx + 1] as u32 + frame[idx + 2] as u32;
                count_r += 3;
            }

            prof_l.push(if count_l > 0 { sum_l as f32 / count_l as f32 } else { 255.0 });
            prof_c.push(if count_c > 0 { sum_c as f32 / count_c as f32 } else { 255.0 });
            prof_r.push(if count_r > 0 { sum_r as f32 / count_r as f32 } else { 255.0 });
        }

        (prof_l, prof_c, prof_r)
    }

    /// Compute 2D overlap SAD and ink-only SAD for a given delta and direction
    fn compute_overlap_metrics(
        &self,
        anchor: &[u8],
        curr: &[u8],
        delta: usize,
        direction: ScrollDirection,
        left_m: usize,
        right_m: usize,
        row_step: usize,
        col_step: usize,
    ) -> (f64, f64, usize) {
        let w = self.width;
        let overlap_h = self.height - delta;

        let mut total_diff = 0u64;
        let mut total_samples = 0u64;

        let mut ink_diff = 0u64;
        let mut ink_samples = 0usize;

        for r in (0..overlap_h).step_by(row_step) {
            let (a_row, c_row) = match direction {
                ScrollDirection::Down => (delta + r, r),
                ScrollDirection::Up => (r, delta + r),
            };

            let a_off = a_row * w * 4;
            let c_off = c_row * w * 4;

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

                // Ink sampling: non-background text/lines (lum < 235)
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
