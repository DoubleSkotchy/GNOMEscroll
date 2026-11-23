use gnomescroll::stitcher::FrameStitcher;

#[test]
fn test_stitcher_accuracy() {
    // Let's create a synthetic high-contrast document with text lines
    let width = 800usize;
    let total_doc_height = 2000usize;
    let mut doc = vec![255u8; width * total_doc_height * 4];

    // Draw horizontal text-like lines every 30px with unique patterns
    for line_idx in 0..(total_doc_height / 30) {
        let y = line_idx * 30 + 10;
        let line_seed = (line_idx * 73 + 17) % 256;
        for r in 0..4 {
            let row_off = (y + r) * width * 4;
            for x in (50..750).step_by(6) {
                let col_seed = (x * 37 + line_seed) % 256;
                if col_seed > 80 {
                    doc[row_off + x * 4] = 20;     // R
                    doc[row_off + x * 4 + 1] = 20; // G
                    doc[row_off + x * 4 + 2] = 20; // B
                    doc[row_off + x * 4 + 3] = 255;
                }
            }
        }
    }

    let view_h = 500usize;
    let mut stitcher = FrameStitcher::new(width as u32, view_h as u32);

    // Discrete scroll positions (simulating wheel ticks)
    let y_scrolls = [0, 45, 120, 260, 410, 620, 850, 1100, 1400];

    for (step, &y) in y_scrolls.iter().enumerate() {
        let start_byte = y * width * 4;
        let end_byte = (y + view_h) * width * 4;
        let frame = &doc[start_byte..end_byte];

        let prev_h = stitcher.current_height;
        let stitched = stitcher.feed_frame(frame);
        if step == 0 {
            assert!(stitched, "Initial frame should be accepted");
        } else {
            let actual_delta = stitcher.current_height - prev_h;
            let expected_delta = y - y_scrolls[step - 1];
            println!("Step {}: expected_delta={}, actual_delta={}, stitched={}", step, expected_delta, actual_delta, stitched);
            assert!(stitched, "Step {} at y={} should be stitched", step, y);
        }
    }

    let (final_w, final_h, count, canvas) = stitcher.finish();
    assert_eq!(final_w, width as u32);
    let expected_h = (y_scrolls.last().unwrap() + view_h) as u32;
    assert_eq!(final_h, expected_h, "Canvas height must match total scrolled height");
    assert_eq!(count, y_scrolls.len(), "All scroll steps should be stitched");

    // Pixel-perfect verification against ground truth doc
    let gt = &doc[0..(expected_h as usize * width * 4)];
    assert_eq!(canvas.len(), gt.len());
    let mut max_diff = 0i32;
    for (&c, &g) in canvas.iter().zip(gt.iter()) {
        let diff = (c as i32 - g as i32).abs();
        if diff > max_diff {
            max_diff = diff;
        }
    }
    assert_eq!(max_diff, 0, "Stitched canvas must be 100% pixel-perfect to ground truth!");
    println!("✅ Verified: Synthetic document test passed with 0 pixel difference!");
}

#[test]
fn test_real_pdf_stitching() {
    let pdf_path = std::path::Path::new("/tmp/toc_page-007.png");
    if !pdf_path.exists() {
        return;
    }

    let img = match image::open(pdf_path) {
        Ok(i) => i.to_rgba8(),
        Err(_) => return,
    };

    let (width, height) = img.dimensions();
    let view_h = 500usize;
    if height < 1200 {
        return;
    }

    let raw = img.into_raw();
    let mut stitcher = FrameStitcher::new(width, view_h as u32);

    // Realistic irregular wheel scroll jumps
    let y_scrolls = [0, 60, 145, 230, 390, 510, 680];

    for (step, &y) in y_scrolls.iter().enumerate() {
        let start_byte = y * width as usize * 4;
        let end_byte = (y + view_h) * width as usize * 4;
        let frame = &raw[start_byte..end_byte];

        let stitched = stitcher.feed_frame(frame);
        if step == 0 {
            assert!(stitched);
        } else {
            assert!(stitched, "Real PDF scroll step {} at y={} must be stitched", step, y);
        }
    }

    let (final_w, final_h, count, canvas) = stitcher.finish();
    let expected_h = (y_scrolls.last().unwrap() + view_h) as u32;
    assert_eq!(final_w, width);
    assert_eq!(final_h, expected_h);
    assert_eq!(count, y_scrolls.len());

    let gt = &raw[0..(expected_h as usize * width as usize * 4)];
    assert_eq!(canvas.len(), gt.len());
    let mut max_diff = 0i32;
    for (&c, &g) in canvas.iter().zip(gt.iter()) {
        let diff = (c as i32 - g as i32).abs();
        if diff > max_diff {
            max_diff = diff;
        }
    }
    assert_eq!(max_diff, 0, "Real PDF stitched canvas must be 100% pixel-perfect!");
    println!("✅ Verified: Real syllabus TOC PDF test passed with 0 pixel difference!");
}

#[test]
fn test_cross_page_jump_rejection() {
    let p7 = std::path::Path::new("/tmp/toc_page-007.png");
    let p10 = std::path::Path::new("/tmp/toc_page-010.png");
    if !p7.exists() || !p10.exists() {
        return;
    }

    let img7 = image::open(p7).unwrap().to_rgba8();
    let img10 = image::open(p10).unwrap().to_rgba8();

    let (w, _h) = img7.dimensions();
    let view_h = 500u32;

    let mut stitcher = FrameStitcher::new(w, view_h);

    // Initial frame from Page 7
    let frame1 = &img7.as_raw()[0..(w as usize * view_h as usize * 4)];
    assert!(stitcher.feed_frame(frame1));
    let initial_h = stitcher.current_height;

    // Next frame is suddenly from Page 10 (user skipped 3 pages!)
    let frame_jump = &img10.as_raw()[0..(w as usize * view_h as usize * 4)];
    let accepted = stitcher.feed_frame(frame_jump);

    // Must be rejected!
    assert!(!accepted, "Cross-page jump with no valid overlap must be rejected!");
    assert_eq!(stitcher.current_height, initial_h, "Canvas must not be corrupted on rejected jump");
    println!("✅ Verified: Discontinuous page jump safely rejected without canvas corruption!");
}


#[test]
fn test_smooth_scrolling_and_pauses() {
    let pdf_path = std::path::Path::new("/tmp/toc_page-007.png");
    if !pdf_path.exists() {
        return;
    }

    let img = match image::open(pdf_path) {
        Ok(i) => i.to_rgba8(),
        Err(_) => return,
    };

    let (width, height) = img.dimensions();
    let view_h = 500usize;
    if height < 1200 {
        return;
    }

    let raw = img.into_raw();
    let mut stitcher = FrameStitcher::new(width, view_h as u32);

    // Realistic trackpad smooth scroll with pause/idle frames in between
    let events: Vec<(usize, bool)> = vec![
        (0, true),    // Frame 0: Initial
        (0, false),   // Frame 0 repeated (idle / cursor blink) -> must NOT stitch
        (0, false),   // Frame 0 repeated again -> must NOT stitch
        (8, true),    // Frame 1: Small scroll 8px
        (8, false),   // Pause
        (22, true),   // Scroll 14px (y=22)
        (45, true),   // Scroll 23px (y=45)
        (45, false),  // Pause
        (90, true),   // Scroll 45px (y=90)
        (160, true),  // Scroll 70px (y=160)
        (260, true),  // Scroll 100px (y=260)
    ];

    let mut actual_stitched = 0usize;
    for (y, should_stitch) in events {
        let start_byte = y * width as usize * 4;
        let end_byte = (y + view_h) * width as usize * 4;
        let frame = &raw[start_byte..end_byte];

        let stitched = stitcher.feed_frame(frame);
        assert_eq!(stitched, should_stitch, "At y={}, expected stitched={} but got {}", y, should_stitch, stitched);
        if stitched {
            actual_stitched += 1;
        }
    }

    assert_eq!(actual_stitched, 7); // 7 unique scroll positions stitched
    let (final_w, final_h, count, canvas) = stitcher.finish();
    assert_eq!(final_w, width);
    assert_eq!(final_h, (260 + view_h) as u32);
    assert_eq!(count, 7);

    // Pixel-perfect check
    let gt = &raw[0..((260 + view_h) * width as usize * 4)];
    assert_eq!(canvas.len(), gt.len());
    let mut max_diff = 0i32;
    for (&c, &g) in canvas.iter().zip(gt.iter()) {
        let diff = (c as i32 - g as i32).abs();
        if diff > max_diff {
            max_diff = diff;
        }
    }
    assert_eq!(max_diff, 0, "Smooth trackpad scroll with pauses must be 100% pixel-perfect!");
    println!("✅ Verified: Smooth scrolling with pauses passed with 0 pixel difference!");
}
