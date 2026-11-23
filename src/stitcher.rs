use rayon::prelude::*;

pub struct FrameStitcher {
    width: usize,
    height: usize,
    canvas: Vec<u8>,
    current_height: usize,
    prev_frame: Option<Vec<u8>>,
    max_delta: usize,
    pub stitched_count: usize,
}

impl FrameStitcher {
    pub fn new(width: u32, height: u32) -> Self {
        let w = width as usize;
        let h = height as usize;
        // Search up to 35% of the frame height per step (e.g. up to 280px on 800h)
        let max_delta = (h as f64 * 0.35) as usize;

        Self {
            width: w,
            height: h,
            canvas: Vec::new(),
            current_height: 0,
            prev_frame: None,
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

        // First frame: initialize canvas
        if self.prev_frame.is_none() {
            self.canvas.extend_from_slice(frame);
            self.current_height = self.height;
            self.prev_frame = Some(frame.to_vec());
            self.stitched_count = 1;
            return true;
        }

        let prev = self.prev_frame.as_ref().unwrap();

        // 1. Fast check if frame is stationary (no scroll, cursor blink, idle)
        if self.is_stationary(prev, frame) {
            return false;
        }

        // 2. Dynamic Feature-Rich Template Selection
        // Instead of hardcoding a region (which might be blank white margin in PDFs),
        // we dynamically find the vertical window in `prev` that has maximum text/contrast!
        let template_h = (self.height as f64 * 0.35) as usize;
        let max_d = self.max_delta.min(self.height / 3);

        let (y0, contrast) = self.find_high_contrast_window(prev, template_h, max_d);

        // If the window has virtually zero contrast (pure white/blank page), skip until content appears
        if contrast < 12.0 {
            self.prev_frame = Some(frame.to_vec());
            return false;
        }

        // Search in parallel across candidate scroll shifts delta \in [1, max_d]
        let best_match: Option<(usize, f64)> = (1..=max_d)
            .into_par_iter()
            .map(|delta| {
                let f2_y0 = y0 - delta;
                let sad_per_pixel = self.compute_template_sad(prev, y0, frame, f2_y0, template_h);
                (delta, sad_per_pixel)
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((best_delta, min_sad)) = best_match {
            // Threshold: average RGB channel error < 22 out of 255
            if min_sad < 22.0 {
                // Verified match! Append newly revealed bottom rows of `frame`
                let start_row = self.height - best_delta;
                let start_byte = start_row * self.width * 4;
                let end_byte = self.height * self.width * 4;

                self.canvas.extend_from_slice(&frame[start_byte..end_byte]);
                self.current_height += best_delta;
                self.prev_frame = Some(frame.to_vec());
                self.stitched_count += 1;
                return true;
            }
        }

        // If sudden jump or match not found, update prev_frame so subsequent frames don't drift
        self.prev_frame = Some(frame.to_vec());
        false
    }

    /// Find the vertical slice [y0 - template_h .. y0] in `prev` that has maximum visual contrast/variance
    fn find_high_contrast_window(&self, prev: &[u8], template_h: usize, max_d: usize) -> (usize, f64) {
        let w = self.width;
        let h = self.height;
        let search_start = template_h + max_d;
        let search_end = h.saturating_sub(10);

        if search_start >= search_end {
            return (h.min(template_h + max_d), 0.0);
        }

        let mut best_y = search_start;
        let mut max_var = -1.0;

        for y in (search_start..search_end).step_by(12) {
            let mut sum = 0u64;
            let mut sum_sq = 0u64;
            let mut count = 0u64;

            for r in (y - template_h..y).step_by(6) {
                let row_off = r * w * 4;
                for c in (0..w).step_by(12) {
                    let val = prev[row_off + c * 4] as u64;
                    sum += val;
                    sum_sq += val * val;
                    count += 1;
                }
            }

            if count > 0 {
                let mean = sum as f64 / count as f64;
                let var = (sum_sq as f64 / count as f64) - (mean * mean);
                if var > max_var {
                    max_var = var;
                    best_y = y;
                }
            }
        }

        (best_y, max_var)
    }

    /// Fast check if consecutive frames are essentially identical (cursor blink, idle)
    fn is_stationary(&self, prev: &[u8], curr: &[u8]) -> bool {
        let w = self.width;
        let h = self.height;
        let row_step = 12;
        let col_step = 16;
        let mut diff_sum = 0u64;
        let mut count = 0u64;

        for r in (h / 4..3 * h / 4).step_by(row_step) {
            let row_off = r * w * 4;
            for c in (w / 4..3 * w / 4).step_by(col_step) {
                let idx = row_off + c * 4;
                let dr = (prev[idx] as i32 - curr[idx] as i32).abs() as u64;
                let dg = (prev[idx + 1] as i32 - curr[idx + 1] as i32).abs() as u64;
                let db = (prev[idx + 2] as i32 - curr[idx + 2] as i32).abs() as u64;
                diff_sum += dr + dg + db;
                count += 3;
            }
        }

        let avg = if count > 0 { diff_sum as f64 / count as f64 } else { 0.0 };
        avg < 1.8
    }

    /// Compute Mean Absolute Difference per color channel between template in prev and candidate in curr
    fn compute_template_sad(
        &self,
        prev: &[u8],
        prev_y0: usize,
        curr: &[u8],
        curr_y0: usize,
        template_h: usize,
    ) -> f64 {
        let w = self.width;
        let row_step = 2;
        let col_step = 4;
        let mut total_diff = 0u64;
        let mut samples = 0u64;

        for r in (0..template_h).step_by(row_step) {
            let p_row_off = (prev_y0 - template_h + r) * w * 4;
            let c_row_off = (curr_y0 - template_h + r) * w * 4;

            for c in (0..w).step_by(col_step) {
                let pi = p_row_off + c * 4;
                let ci = c_row_off + c * 4;

                let dr = (prev[pi] as i32 - curr[ci] as i32).abs() as u64;
                let dg = (prev[pi + 1] as i32 - curr[pi + 1] as i32).abs() as u64;
                let db = (prev[pi + 2] as i32 - curr[ci + 2] as i32).abs() as u64;
                total_diff += dr + dg + db;
                samples += 3;
            }
        }

        if samples > 0 {
            total_diff as f64 / samples as f64
        } else {
            255.0
        }
    }

    /// Return the finalized RGBA image dimensions, total stitched frame count, and pixel buffer
    pub fn finish(self) -> (u32, u32, usize, Vec<u8>) {
        (self.width as u32, self.current_height as u32, self.stitched_count, self.canvas)
    }
}
