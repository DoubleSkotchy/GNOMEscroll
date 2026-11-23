use rayon::prelude::*;

pub struct FrameStitcher {
    width: usize,
    height: usize,
    /// The running stitched image RGBA data
    canvas: Vec<u8>,
    current_height: usize,
    /// The previous frame's RGBA data
    prev_frame: Option<Vec<u8>>,
    /// Precomputed row luminance of previous frame for fast 1D matching
    prev_row_luma: Option<Vec<u32>>,
    /// Minimum scroll step in pixels to register as new content
    min_scroll: usize,
    /// Maximum scroll step between consecutive frames (usually < height * 0.9)
    max_scroll: usize,
    /// Number of stitched frames
    pub stitched_count: usize,
}

impl FrameStitcher {
    pub fn new(width: u32, height: u32) -> Self {
        let w = width as usize;
        let h = height as usize;
        let max_scroll = (h as f64 * 0.90) as usize;

        Self {
            width: w,
            height: h,
            canvas: Vec::new(),
            current_height: 0,
            prev_frame: None,
            prev_row_luma: None,
            min_scroll: 3,
            max_scroll,
            stitched_count: 0,
        }
    }

    /// Process a new incoming RGBA frame from screencast
    pub fn feed_frame(&mut self, frame: &[u8]) -> bool {
        let expected_size = self.width * self.height * 4;
        if frame.len() != expected_size {
            return false;
        }

        // First frame: initialize canvas
        if self.prev_frame.is_none() {
            self.canvas.extend_from_slice(frame);
            self.current_height = self.height;
            self.prev_row_luma = Some(self.compute_row_luma(frame));
            self.prev_frame = Some(frame.to_vec());
            self.stitched_count = 1;
            return true;
        }

        let prev = self.prev_frame.as_ref().unwrap();
        let prev_luma = self.prev_row_luma.as_ref().unwrap();
        let curr_luma = self.compute_row_luma(frame);

        // Check for identical/near-identical frame (no scroll)
        if self.is_frame_unchanged(prev_luma, &curr_luma) {
            return false;
        }

        // Find best vertical shift delta_y (how many pixels the content moved UP)
        if let Some(delta_y) = self.find_vertical_shift(prev, prev_luma, frame, &curr_luma) {
            if delta_y >= self.min_scroll && delta_y <= self.max_scroll {
                // Append the newly revealed bottom rows (from height - delta_y to height)
                let start_row = self.height - delta_y;
                let start_byte = start_row * self.width * 4;
                let end_byte = self.height * self.width * 4;

                self.canvas.extend_from_slice(&frame[start_byte..end_byte]);
                self.current_height += delta_y;
                self.prev_frame = Some(frame.to_vec());
                self.prev_row_luma = Some(curr_luma);
                self.stitched_count += 1;
                return true;
            }
        }

        false
    }

    /// Compute horizontal average luminance for each row
    /// Using standard Rec.601 luma weights: Y = (299*R + 587*G + 114*B) / 1000
    fn compute_row_luma(&self, frame: &[u8]) -> Vec<u32> {
        let w = self.width;
        let h = self.height;
        // Sample middle 60% of columns to avoid scrollbars and fixed side margins
        let col_start = (w as f64 * 0.15) as usize;
        let col_end = (w as f64 * 0.85) as usize;
        let col_count = col_end - col_start;
        if col_count == 0 {
            return vec![0; h];
        }

        (0..h)
            .into_par_iter()
            .map(|row| {
                let row_offset = row * w * 4;
                let mut sum_luma = 0u64;
                // Sample every 2nd pixel for extra speed
                let step = 2;
                let mut samples = 0u64;
                for col in (col_start..col_end).step_by(step) {
                    let idx = row_offset + col * 4;
                    let r = frame[idx] as u64;
                    let g = frame[idx + 1] as u64;
                    let b = frame[idx + 2] as u64;
                    sum_luma += (299 * r + 587 * g + 114 * b) / 1000;
                    samples += 1;
                }
                if samples > 0 {
                    (sum_luma / samples) as u32
                } else {
                    0
                }
            })
            .collect()
    }

    /// Fast check if consecutive frames are essentially identical
    fn is_frame_unchanged(&self, prev_luma: &[u32], curr_luma: &[u32]) -> bool {
        let mut diff_sum = 0u64;
        let step = 4;
        let mut count = 0u64;
        for i in (0..self.height).step_by(step) {
            let d = (prev_luma[i] as i64 - curr_luma[i] as i64).abs() as u64;
            diff_sum += d;
            count += 1;
        }
        let avg_diff = if count > 0 { diff_sum / count } else { 0 };
        avg_diff < 2 // Less than ~1% average luminance change
    }

    /// Find vertical pixel shift delta_y using 1D correlation followed by 2D block verification
    fn find_vertical_shift(
        &self,
        prev_frame: &[u8],
        prev_luma: &[u32],
        curr_frame: &[u8],
        curr_luma: &[u32],
    ) -> Option<usize> {
        let h = self.height;
        let min_d = self.min_scroll;
        let max_d = self.max_scroll;

        // Stage 1: Fast 1D SAD over row luminance
        // prev[row + delta] should match curr[row]
        let best_shifts: Vec<(usize, u64)> = (min_d..=max_d)
            .into_par_iter()
            .map(|delta| {
                let overlap = h - delta;
                let sample_step = 3;
                let mut sad = 0u64;
                let mut count = 0u64;
                for r in (0..overlap).step_by(sample_step) {
                    let prev_val = prev_luma[r + delta] as i64;
                    let curr_val = curr_luma[r] as i64;
                    sad += (prev_val - curr_val).abs() as u64;
                    count += 1;
                }
                let score = if count > 0 { (sad * 1000) / count } else { u64::MAX };
                (delta, score)
            })
            .collect();

        // Find candidate shifts with lowest 1D error
        let mut candidates = best_shifts;
        candidates.sort_by_key(|&(_, score)| score);

        // Stage 2: 2D pixel verification on top 3 candidates
        let w = self.width;
        let col_start = (w as f64 * 0.20) as usize;
        let col_end = (w as f64 * 0.80) as usize;

        for &(candidate_delta, score1d) in candidates.iter().take(4) {
            if score1d > 25000 {
                // If 1D difference is too high, it's not a match
                continue;
            }

            let overlap = h - candidate_delta;
            // Verify with 2D sample grid
            let mut pixel_sad = 0u64;
            let mut samples = 0u64;
            let row_step = 8;
            let col_step = 16;

            for r in (0..overlap).step_by(row_step) {
                let prev_row_off = (r + candidate_delta) * w * 4;
                let curr_row_off = r * w * 4;

                for c in (col_start..col_end).step_by(col_step) {
                    let pi = prev_row_off + c * 4;
                    let ci = curr_row_off + c * 4;

                    let dr = (prev_frame[pi] as i32 - curr_frame[ci] as i32).abs() as u64;
                    let dg = (prev_frame[pi + 1] as i32 - curr_frame[ci + 1] as i32).abs() as u64;
                    let db = (prev_frame[pi + 2] as i32 - curr_frame[ci + 2] as i32).abs() as u64;
                    pixel_sad += dr + dg + db;
                    samples += 3;
                }
            }

            let avg_pixel_error = if samples > 0 { pixel_sad / samples } else { 255 };
            // If average RGB difference per channel is < 18 (out of 255), this is a confirmed match!
            if avg_pixel_error < 18 {
                return Some(candidate_delta);
            }
        }

        None
    }

    /// Return the finalized RGBA image dimensions, total stitched frame count, and pixel buffer
    pub fn finish(self) -> (u32, u32, usize, Vec<u8>) {
        (self.width as u32, self.current_height as u32, self.stitched_count, self.canvas)
    }
}
