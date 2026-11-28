use std::time::Duration;

use super::*;
use crate::compositor::graphics::lshape::{apply::*, shape::*};
use crate::frame::{PixelFormat, VideoFrame};

#[test]
fn cancelled_owner_cannot_publish_an_lshape_after_preparation() {
    use std::sync::{
        Barrier,
        atomic::{AtomicBool, Ordering},
    };

    let control = LShapeControl::default();
    let original = crate::GraphicSpec {
        layers: Vec::new(),
        picture: Some(crate::PictureWindow {
            x: 25.0,
            y: 0.0,
            width: 75.0,
            height: 75.0,
        }),
    };
    let next = crate::GraphicSpec {
        layers: Vec::new(),
        picture: Some(crate::PictureWindow {
            x: 50.0,
            y: 50.0,
            width: 50.0,
            height: 50.0,
        }),
    };
    control.set_template(&original, 320, 180).unwrap();
    let original_window = control.lock().graphic.as_ref().unwrap().window;
    let allowed = Arc::new(AtomicBool::new(true));
    let prepared = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let worker_control = control.clone();
    let worker_allowed = allowed.clone();
    let worker_prepared = prepared.clone();
    let worker_release = release.clone();
    let worker = std::thread::spawn(move || {
        let mut calls = 0;
        worker_control
            .set_template_controlled_guarded(&next, usize::MAX, (320, 180), || {
                calls += 1;

                if calls == 2 {
                    worker_prepared.wait();
                    worker_release.wait();
                }

                worker_allowed.load(Ordering::SeqCst).then_some(())
            })
            .unwrap();
    });
    prepared.wait();
    let frames_unlocked = control.0.try_lock().is_ok();
    allowed.store(false, Ordering::SeqCst);
    release.wait();
    worker.join().unwrap();
    assert!(frames_unlocked, "preparation must leave frames unlocked");
    assert_eq!(
        control.lock().graphic.as_ref().unwrap().window,
        original_window
    );
}

fn scrolling_spec() -> crate::GraphicSpec {
    crate::GraphicSpec {
        picture: Some(crate::PictureWindow {
            x: 25.0,
            y: 0.0,
            width: 75.0,
            height: 75.0,
        }),
        layers: vec![crate::GraphicLayerSpec {
            content: crate::GraphicContent::Text {
                text: "Scrolling artwork".to_string(),
                scroll: Some(crate::GraphicTextScroll {
                    direction: crate::ScrollDirection::Left,
                    speed: 80.0,
                    gap: 40,
                }),
                font_family: None,
                font_size: 6.0,
                bold: false,
                color: crate::RgbaColor::opaque(255, 255, 255),
                align: crate::BlockAlign::Left,
                fill: None,
            },
            x: 5.0,
            y: 80.0,
            width: 90.0,
            height: 10.0,
            opacity: 1.0,
            radius: 0.0,
            animation_in: crate::GraphicAnimation::None,
            animation_out: crate::GraphicAnimation::None,
            animation_ms: 0,
            delay_ms: 0,
        }],
    }
}

#[test]
fn refreshing_scrolling_artwork_preserves_its_motion_epoch() {
    let control = LShapeControl::default();
    let spec = scrolling_spec();
    control.set_template(&spec, 320, 180).unwrap();
    let epoch = control
        .lock()
        .graphic
        .as_ref()
        .unwrap()
        .animated_art
        .as_ref()
        .unwrap()
        .layers[0]
        .prepared_at;
    control.set_template(&spec, 320, 180).unwrap();
    let next = control
        .lock()
        .graphic
        .as_ref()
        .unwrap()
        .animated_art
        .as_ref()
        .unwrap()
        .layers[0]
        .prepared_at;
    assert_eq!(next, epoch);
}

#[test]
fn temporary_picture_refresh_inherits_nested_scroll_epoch() {
    let control = crate::GraphicsControl::default();
    let spec = scrolling_spec();
    let mut initial = render_graphic(&spec, 320, 180).unwrap();
    let epoch = Instant::now() - Duration::from_secs(5);
    let picture = Arc::get_mut(initial.picture.as_mut().unwrap()).unwrap();
    Arc::get_mut(picture.animated_art.as_mut().unwrap())
        .unwrap()
        .layers[0]
        .prepared_at = epoch;
    let id = control.take(initial, Some(Duration::from_secs(30)));
    let entered = control.state().on_air[0].taken_at;
    control.take_out(id);
    let leaving = control.state().on_air[0].taken_out_at;
    assert!(control.update_graphic(id, render_graphic(&spec, 320, 180).unwrap()));
    let state = control.state();
    let take = &state.on_air[0];
    let art = take
        .graphic
        .picture
        .as_ref()
        .unwrap()
        .animated_art
        .as_ref()
        .unwrap();
    assert_eq!(art.layers[0].prepared_at, epoch);
    assert_eq!(take.taken_at, entered);
    assert_eq!(take.taken_out_at, leaving);
    assert_eq!(take.out_after, Some(Duration::from_secs(30)));
}

#[test]
fn model_override_refresh_prepares_without_frame_lock_and_keeps_newer_controls() {
    let control = LShapeControl::default();
    let spec = scrolling_spec();
    control.set_template(&spec, 320, 180).unwrap();
    {
        let mut state = control.lock();
        let graphic = state.graphic.as_ref().unwrap().clone();
        state.replacement = Some(("old".to_string(), graphic.clone()));
        state.replacement_image = Some("old.png".to_string());
        state.replacement_uses_model = true;
        state.clip = Some((
            LShapeConfig {
                path: "clip.png".to_string(),
                scale: 60.0,
                corner: LShapeCorner::TopRight,
            },
            graphic,
        ));
        state.clip_uses_model = true;
    }
    let next = render_graphic(&spec, 320, 180).unwrap().picture;
    let mut preparations = 0;
    control
        .set_rendered_graphic_with(
            next,
            &spec,
            0,
            control.begin_model_update(),
            |_, _, width, height| {
                preparations += 1;
                assert!(
                    control.0.try_lock().is_ok(),
                    "override preparation holds the frame mutex"
                );
                assert!(control.frame(1.0).is_some());
                if preparations == 1 {
                    control.reset();
                    control.set_clip_graphic(None, width, height).unwrap();
                    control
                        .place(Some(LShapePlacement {
                            corner: LShapeCorner::BottomLeft,
                            scale: Some(60.0),
                        }))
                        .unwrap();
                }

                Ok(render_graphic(&spec, width, height)
                    .unwrap()
                    .picture
                    .unwrap()
                    .as_ref()
                    .clone())
            },
        )
        .unwrap();
    let state = control.lock();
    assert!(preparations >= 1);
    assert!(state.replacement.is_none());
    assert!(state.clip.is_none());
    assert_eq!(state.graphic.as_ref().unwrap().window.x, 0);
    assert!(state.graphic.as_ref().unwrap().window.y > 0);
}

#[test]
fn older_lshape_override_preparation_cannot_publish_after_a_model_clear() {
    let control = LShapeControl::default();
    let spec = scrolling_spec();
    control.set_template(&spec, 320, 180).unwrap();
    {
        let mut state = control.lock();
        state.replacement = Some(("old".to_string(), state.graphic.as_ref().unwrap().clone()));
        state.replacement_image = Some("old.png".to_string());
        state.replacement_uses_model = true;
    }
    let next = render_graphic(&spec, 320, 180).unwrap().picture;
    control
        .set_rendered_graphic_with(
            next,
            &spec,
            0,
            control.begin_model_update(),
            |_, _, width, height| {
                control.set_graphic(None, width, height).unwrap();

                Ok(render_graphic(&spec, width, height)
                    .unwrap()
                    .picture
                    .unwrap()
                    .as_ref()
                    .clone())
            },
        )
        .unwrap();
    let state = control.lock();
    assert!(state.graphic.is_none());
    assert!(state.model.is_none());
}

#[test]
fn loading_a_model_preserves_free_picture_geometry_and_on_air_state() {
    let control = LShapeControl::default();
    let spec = crate::GraphicSpec {
        picture: Some(crate::PictureWindow {
            x: 10.0,
            y: 20.0,
            width: 70.0,
            height: 50.0,
        }),
        layers: Vec::new(),
    };
    control.set_template(&spec, 640, 360).unwrap();
    assert!(!control.is_on());
    let (graphic, _) = control.frame(0.0).unwrap();
    assert_eq!(
        graphic.window,
        Rect {
            x: 64,
            y: 72,
            width: 448,
            height: 180
        }
    );
    control.on();
    control.set_template(&spec, 640, 360).unwrap();
    assert!(control.is_on());
    control
        .place(Some(LShapePlacement {
            corner: LShapeCorner::BottomLeft,
            scale: None,
        }))
        .unwrap();
    control.set_template(&spec, 640, 360).unwrap();
    let (graphic, _) = control.frame(0.0).unwrap();
    assert_eq!(graphic.window.x, 0);
    assert_eq!(graphic.window.y, 108);
    control.place(None).unwrap();
    let (graphic, _) = control.frame(0.0).unwrap();
    assert_eq!(
        graphic.window,
        Rect {
            x: 64,
            y: 72,
            width: 448,
            height: 180
        }
    );
}

#[test]
fn model_art_replacement_and_clip_art_inherit_free_windows_without_changing_sdk_placement() {
    let control = LShapeControl::default();
    let spec = crate::GraphicSpec {
        layers: Vec::new(),
        picture: Some(crate::PictureWindow {
            x: 10.0,
            y: 10.0,
            width: 70.0,
            height: 50.0,
        }),
    };
    control.set_template(&spec, 640, 360).unwrap();
    let config = LShapeConfig {
        path: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests_assets/storage/logo.png")
            .to_string_lossy()
            .to_string(),
        scale: 70.0,
        corner: LShapeCorner::TopRight,
    };
    let free = Rect {
        x: 64,
        y: 36,
        width: 448,
        height: 180,
    };
    control
        .replace_model_art("replacement", &config, 640, 360)
        .unwrap();
    assert_eq!(control.frame(0.0).unwrap().0.window, free);
    assert!(
        !control.is_on(),
        "loading artwork does not enter the L-shape"
    );
    control
        .place(Some(LShapePlacement {
            corner: LShapeCorner::BottomLeft,
            scale: Some(60.0),
        }))
        .unwrap();
    assert_eq!(
        control.frame(0.0).unwrap().0.window,
        Rect {
            x: 0,
            y: 144,
            width: 384,
            height: 216
        }
    );
    control.place(None).unwrap();
    assert_eq!(control.frame(0.0).unwrap().0.window, free);
    control.reset();
    control.set_clip_model_art(Some(&config), 640, 360).unwrap();
    assert_eq!(control.frame(1.0).unwrap().0.window, free);
    control.set_clip_graphic(Some(&config), 640, 360).unwrap();
    assert_eq!(
        control.frame(1.0).unwrap().0.window,
        Rect {
            x: 192,
            y: 0,
            width: 448,
            height: 252
        }
    );
    control.set_clip_model_art(Some(&config), 640, 360).unwrap();
    assert_eq!(control.frame(1.0).unwrap().0.window, free);
    control.set_clip_model_art(None, 640, 360).unwrap();
    assert_eq!(control.frame(1.0).unwrap().0.window, free);
}

fn graphic(corner: LShapeCorner) -> LShapeGraphic {
    let mut background = VideoFrame::new(PixelFormat::Yuv420p, 1920, 1080);
    for plane in 0..3 {
        background
            .data_mut(plane)
            .fill(if plane == 0 { 200 } else { 128 });
    }
    let config = LShapeConfig {
        path: String::new(),
        scale: 75.0,
        corner,
    };

    let home = window(&config, 1920, 1080);

    LShapeGraphic {
        background: Arc::new(background),
        animated_art: None,
        window: home,
        home,
    }
}

#[test]
fn the_window_sits_in_its_corner() {
    let config = |corner| LShapeConfig {
        path: String::new(),
        scale: 75.0,
        corner,
    };

    assert_eq!(
        window(&config(LShapeCorner::TopRight), 1920, 1080),
        Rect {
            x: 480,
            y: 0,
            width: 1440,
            height: 810
        }
    );
    assert_eq!(
        window(&config(LShapeCorner::BottomLeft), 1920, 1080),
        Rect {
            x: 0,
            y: 270,
            width: 1440,
            height: 810
        }
    );
    assert_eq!(
        LShapeCorner::parse("bottom-right"),
        Some(LShapeCorner::BottomRight)
    );
    assert_eq!(LShapeCorner::parse("middle"), None);
}

#[test]
fn a_corner_reads_back_as_its_name() {
    for corner in [
        LShapeCorner::TopRight,
        LShapeCorner::TopLeft,
        LShapeCorner::BottomRight,
        LShapeCorner::BottomLeft,
    ] {
        assert_eq!(LShapeCorner::parse(&corner.to_string()), Some(corner));
    }
    assert_eq!(LShapeCorner::parse("50%:50%"), None);
}

#[test]
fn a_move_from_outside_places_every_graphic_until_it_is_undone() {
    let control = LShapeControl::default();
    control.lock().graphic = Some(Arc::new(graphic(LShapeCorner::TopRight)));
    let window = |control: &LShapeControl| control.frame(1.0).unwrap().0.window;
    let home = window(&control);

    control
        .place(Some(LShapePlacement {
            corner: LShapeCorner::BottomLeft,
            scale: Some(50.0),
        }))
        .unwrap();
    assert_eq!(
        window(&control),
        Rect {
            x: 0,
            y: 540,
            width: 960,
            height: 540
        }
    );

    // The size stays the graphic's when only the corner is given.
    control
        .place(Some(LShapePlacement {
            corner: LShapeCorner::TopLeft,
            scale: None,
        }))
        .unwrap();
    assert_eq!(window(&control).width, home.width);
    assert_eq!((window(&control).x, window(&control).y), (0, 0));

    assert!(
        control
            .place(Some(LShapePlacement {
                corner: LShapeCorner::TopLeft,
                scale: Some(10.0),
            }))
            .is_err()
    );

    control.place(None).unwrap();
    assert_eq!(window(&control), home);
}

#[test]
fn the_picture_moves_from_the_full_screen_to_the_window() {
    let graphic = graphic(LShapeCorner::TopRight);

    assert_eq!(
        graphic.rect_at(0.0),
        Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080
        }
    );
    assert_eq!(graphic.rect_at(1.0), graphic.window);
    let half = graphic.rect_at(0.5);
    assert!(half.width < 1920 && half.width > 1440, "{half:?}");
    assert_eq!(half.y, 0);
}

#[test]
fn the_frame_is_drawn_into_the_window_over_the_graphic() {
    let graphic = graphic(LShapeCorner::TopRight);
    let mut frame = VideoFrame::new(PixelFormat::Yuv420p, 1920, 1080);
    for plane in 0..3 {
        frame
            .data_mut(plane)
            .fill(if plane == 0 { 50 } else { 128 });
    }
    frame.set_pts(Some(7));
    let mut scaler = LShapeScaler::default();

    apply_lshape(&mut frame, &graphic, 1.0, &mut scaler).unwrap();

    let luma = |x: usize, y: usize| frame.data(0)[y * frame.stride(0) + x];
    // The band shows the graphic, the window the picture.
    assert_eq!(luma(100, 500), 200);
    assert_eq!(luma(100, 1000), 200);
    assert_eq!(luma(1000, 400), 50);
    assert_eq!(frame.pts(), Some(7));

    // Off: untouched.
    let mut plain = VideoFrame::new(PixelFormat::Yuv420p, 1920, 1080);
    plain.data_mut(0).fill(50);
    apply_lshape(&mut plain, &graphic, 0.0, &mut scaler).unwrap();
    assert_eq!(plain.data(0)[0], 50);
}

#[test]
fn switching_animates_both_ways() {
    let control = LShapeControl::default();

    control.on();
    assert!(control.is_on());
    assert!(control.level_after(Duration::ZERO) < 0.01);
    assert!(
        (control.level_after(Duration::from_secs_f64(LSHAPE_SWITCH_SECONDS / 2.0)) - 0.5).abs()
            < 0.01
    );
    assert_eq!(control.level_after(Duration::from_secs(2)), 1.0);

    control.lock().switched_at = None;
    control.off();
    assert_eq!(control.level_after(Duration::ZERO), 1.0);
    assert_eq!(control.level_after(Duration::from_secs(2)), 0.0);
    // No graphic: nothing to draw.
    assert!(control.frame(1.0).is_none());
}

#[test]
fn a_swapped_or_clip_graphic_takes_the_channels_place() {
    let control = LShapeControl::default();
    let channel = Arc::new(graphic(LShapeCorner::TopRight));
    let other = Arc::new(graphic(LShapeCorner::BottomLeft));
    let clip = Arc::new(graphic(LShapeCorner::TopLeft));
    control.lock().graphic = Some(channel.clone());

    let (shown, level) = control.frame(0.0).unwrap();
    assert!(Arc::ptr_eq(&shown, &channel));
    assert_eq!(level, 0.0);

    control.lock().replacement = Some(("sponsor.png".to_string(), other.clone()));
    assert_eq!(control.replacement().as_deref(), Some("sponsor.png"));
    assert!(Arc::ptr_eq(&control.frame(0.0).unwrap().0, &other));

    // A marked stretch uses the clip's graphic.
    let config = LShapeConfig {
        path: String::new(),
        scale: 75.0,
        corner: LShapeCorner::TopLeft,
    };
    control.lock().clip = Some((config, clip.clone()));
    let (shown, level) = control.frame(0.6).unwrap();
    assert!(Arc::ptr_eq(&shown, &clip));
    assert_eq!(level, 0.6);
    // Without marks, the swapped one.
    assert!(Arc::ptr_eq(&control.frame(0.0).unwrap().0, &other));

    control.reset();
    assert!(control.replacement().is_none());
    control.set_clip_graphic(None, 1920, 1080).unwrap();
    assert!(Arc::ptr_eq(&control.frame(0.6).unwrap().0, &channel));
    assert!(
        control
            .set_clip_graphic(
                Some(&LShapeConfig {
                    path: "/missing.png".to_string(),
                    scale: 75.0,
                    corner: LShapeCorner::TopLeft,
                }),
                1920,
                1080
            )
            .is_err()
    );
}

#[test]
fn the_logo_goes_off_before_the_picture_shrinks() {
    assert_eq!(lshape_phases(0.0), (0.0, 0.0));
    // A third of the way: the logo is gone, the picture not moved yet.
    let (logo, picture) = lshape_phases(LSHAPE_LOGO_SECONDS / LSHAPE_SWITCH_SECONDS);
    assert!((logo - 1.0).abs() < 1e-9 && picture.abs() < 1e-9);
    let (logo, picture) = lshape_phases(0.2);
    assert!(logo > 0.5 && logo < 1.0 && picture == 0.0);
    let (logo, picture) = lshape_phases(0.75);
    assert!((logo - 1.0).abs() < 1e-9 && picture > 0.5 && picture < 1.0);
    assert_eq!(lshape_phases(1.0), (1.0, 1.0));
}

#[test]
fn marked_stretches_ease_in_and_out() {
    let spans = [(100, 200)];

    assert_eq!(marked_level(&spans, 50, 20), 0.0);
    assert_eq!(marked_level(&spans, 100, 20), 0.0);
    assert!((marked_level(&spans, 110, 20) - 0.5).abs() < 0.01);
    assert_eq!(marked_level(&spans, 150, 20), 1.0);
    assert!((marked_level(&spans, 190, 20) - 0.5).abs() < 0.01);
    assert_eq!(marked_level(&spans, 201, 20), 0.0);
    assert_eq!(marked_level(&spans, 150, 0), 1.0);
}
