use std::time::{Duration, Instant};

use super::{LogoControl, LogoOverride, LogoTransition, LogoTransitions};
use crate::{
    compositor::graphics::logo::position::{logo_dimensions, logo_position},
    utils::config::LogoConfig,
};

fn logo_asset() -> LogoConfig {
    LogoConfig {
        path: concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests_assets/storage/logo.png"
        )
        .to_string(),
        scale: Some("10%:-1".to_string()),
        opacity: 1.0,
        position: "W-w-12:12".to_string(),
    }
}

fn scrolling_model_spec() -> crate::GraphicSpec {
    use crate::{GraphicAnimation, GraphicContent, GraphicLayerSpec, GraphicSpec, RgbaColor};
    let image = GraphicLayerSpec {
        content: GraphicContent::Image {
            path: "unavailable.png".into(),
        },
        x: 10.0,
        y: 10.0,
        width: 20.0,
        height: 20.0,
        opacity: 1.0,
        radius: 0.0,
        animation_in: GraphicAnimation::SlideLeft,
        animation_out: GraphicAnimation::Wipe,
        animation_ms: 1000,
        delay_ms: 200,
    };
    GraphicSpec {
        picture: None,
        layers: vec![
            image.clone(),
            GraphicLayerSpec {
                content: GraphicContent::Text {
                    text: "Scrolling model".to_string(),
                    scroll: Some(crate::GraphicTextScroll {
                        direction: crate::ScrollDirection::Left,
                        speed: 80.0,
                        gap: 40,
                    }),
                    font_family: None,
                    font_size: 6.0,
                    bold: false,
                    color: RgbaColor::opaque(255, 255, 255),
                    align: crate::BlockAlign::Left,
                    fill: None,
                },
                y: 70.0,
                width: 80.0,
                ..image
            },
        ],
    }
}

fn synthetic_model(
    spec: &crate::GraphicSpec,
    primary: usize,
    width: u32,
    height: u32,
) -> anyhow::Result<super::LogoOverlay> {
    // Prepared test images have no dependency on native image I/O.
    let mut drawable = spec.clone();
    for layer in &mut drawable.layers {
        if matches!(layer.content, crate::GraphicContent::Image { .. }) {
            layer.content = crate::GraphicContent::Box {
                fill: crate::RgbaColor::opaque(255, 255, 255),
            };
        }
    }
    let mut overlay = super::LogoOverlay::from_model(&drawable, primary, width, height)?;
    std::sync::Arc::get_mut(overlay.model.as_mut().unwrap())
        .unwrap()
        .spec = spec.clone();

    Ok(overlay)
}

#[test]
fn moved_models_share_prepared_pixels_scroll_epoch_and_layer_animations() {
    let spec = scrolling_model_spec();
    let base = synthetic_model(&spec, 0, 320, 180).unwrap();
    let moved = base.moved_to("80:40", 320, 180).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        base.model.as_ref().unwrap(),
        moved.model.as_ref().unwrap()
    ));
    assert_eq!(moved.model_offset, (48, 22));
    assert_eq!(moved.x, 80);
    assert_eq!(moved.y, 40);
    let returned = moved.moved_to("32:18", 320, 180).unwrap();
    assert_eq!(returned.model_offset, (0, 0));
    let model = returned.model.as_ref().unwrap();
    assert!(model.graphic.layers[1].scroll.is_some());
    assert_eq!(
        model.graphic.layers[0].animation_in,
        crate::GraphicAnimation::SlideLeft
    );
    assert_eq!(model.graphic.layers[0].delay, Duration::from_millis(200));
}

#[test]
fn moving_a_gif_model_keeps_its_decoded_animation_without_reopening_the_source() {
    let mut spec = scrolling_model_spec();
    spec.layers[0].content = crate::GraphicContent::Image {
        path: concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests_assets/graphics.gif"
        )
        .into(),
    };
    let mut base = super::LogoOverlay::from_model(&spec, 0, 320, 180).unwrap();
    let model = std::sync::Arc::get_mut(base.model.as_mut().unwrap()).unwrap();
    assert!(model.graphic.layers[0].image_animation.is_some());
    model.spec.layers[0].content = crate::GraphicContent::Image {
        path: "missing-after-preparation.gif".into(),
    };
    let moved = base.moved_to("80:40", 320, 180).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        base.model.as_ref().unwrap(),
        moved.model.as_ref().unwrap()
    ));
}

#[test]
fn logo_refresh_keeps_independent_motion_epochs_for_definition_and_overrides() {
    use std::sync::Arc;
    let control = LogoControl::default();
    let spec = scrolling_model_spec();
    let epochs = [5, 7, 9].map(|seconds| Instant::now() - Duration::from_secs(seconds));
    let overlays: Vec<_> = epochs
        .iter()
        .map(|epoch| {
            let mut overlay = synthetic_model(&spec, 0, 320, 180).unwrap();
            for layer in &mut Arc::get_mut(overlay.model.as_mut().unwrap())
                .unwrap()
                .graphic
                .layers
            {
                layer.prepared_at = *epoch;
            }
            Arc::new(overlay)
        })
        .collect();
    {
        let mut state = control.lock();
        state.model_definition = Some(overlays[0].clone());
        state.configured = Some(Some(overlays[0].clone()));
        state.clip_image = Some(("clip.png".to_string(), overlays[1].clone()));
        state.replacement = Some(("external".to_string(), overlays[2].clone()));
    }
    let mut next = spec.clone();
    next.layers[1].opacity = 0.7;
    control
        .set_model_with(
            super::LogoModelUpdate {
                spec: Some(&next),
                primary: 0,
                clip_animation_ms: None,
                enabled: true,
                size: (320, 180),
                animate: false,
            },
            synthetic_model,
        )
        .unwrap();
    let state = control.lock();
    let images = [
        state.model_definition.as_ref().unwrap(),
        &state.clip_image.as_ref().unwrap().1,
        &state.replacement.as_ref().unwrap().1,
    ];
    for (image, epoch) in images.into_iter().zip(epochs) {
        assert_eq!(
            image.model.as_ref().unwrap().graphic.layers[1].prepared_at,
            epoch
        );
    }
}

#[test]
fn override_refresh_releases_frame_mutex_and_discards_changed_inputs() {
    use std::sync::Arc;
    let control = LogoControl::default();
    let spec = scrolling_model_spec();
    let old = Arc::new(synthetic_model(&spec, 0, 320, 180).unwrap());
    {
        let mut state = control.lock();
        state.model_definition = Some(old.clone());
        state.configured = Some(Some(old.clone()));
        state.clip_image = Some(("clip.png".to_string(), old.clone()));
        state.replacement = Some(("external".to_string(), old));
    }
    let mut next = spec.clone();
    next.layers[1].opacity = 0.7;
    let mut preparations = 0;
    control
        .set_model_with(
            super::LogoModelUpdate {
                spec: Some(&next),
                primary: 0,
                clip_animation_ms: None,
                enabled: true,
                size: (320, 180),
                animate: false,
            },
            |spec, primary, width, height| {
                preparations += 1;
                assert!(
                    control.0.try_lock().is_ok(),
                    "preparation holds the frame mutex"
                );
                let _ = control.frame();
                if preparations == 2 {
                    control.set_clip_image(None, width, height).unwrap();
                    control.show();
                }

                synthetic_model(spec, primary, width, height)
            },
        )
        .unwrap();
    assert!(preparations >= 3);
    let state = control.lock();
    assert!(state.clip_image.is_none());
    assert!(state.replacement.is_none());
    assert_eq!(
        state
            .model_definition
            .as_ref()
            .unwrap()
            .model
            .as_ref()
            .unwrap()
            .spec,
        next
    );
}

#[test]
fn cancelled_owner_cannot_publish_a_logo_after_preparation() {
    use crate::{GraphicContent, GraphicSpec};
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    };

    let control = LogoControl::default();
    let original = GraphicSpec {
        layers: Vec::new(),
        picture: None,
    };
    let mut next = scrolling_model_spec();
    next.layers[0].content = GraphicContent::Box {
        fill: crate::RgbaColor::opaque(255, 0, 0),
    };
    control
        .set_model(Some(&original), 0, None, true, 320, 180)
        .unwrap();
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
            .refresh_model_guarded(Some(&next), 0, None, true, (320, 180), || {
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
    let state = control.lock();
    assert_eq!(
        state
            .model_definition
            .as_ref()
            .unwrap()
            .model
            .as_ref()
            .unwrap()
            .spec,
        original
    );
}

#[test]
fn older_logo_preparation_cannot_republish_after_a_newer_model_clear() {
    let control = LogoControl::default();
    let spec = scrolling_model_spec();
    control
        .set_model_with(
            super::LogoModelUpdate {
                spec: Some(&spec),
                primary: 0,
                clip_animation_ms: None,
                enabled: true,
                size: (320, 180),
                animate: false,
            },
            |spec, primary, width, height| {
                control
                    .set_model(None, 0, None, false, width, height)
                    .unwrap();

                synthetic_model(spec, primary, width, height)
            },
        )
        .unwrap();
    assert!(control.lock().model_definition.is_none());
}

#[test]
fn dynamic_role_refresh_preserves_the_active_fade_and_hidden_state() {
    let control = LogoControl::default();
    let spec = crate::GraphicSpec {
        picture: None,
        layers: vec![crate::GraphicLayerSpec {
            content: crate::GraphicContent::Box {
                fill: crate::RgbaColor {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
            },
            x: 80.0,
            y: 5.0,
            width: 10.0,
            height: 10.0,
            opacity: 1.0,
            radius: 0.0,
            animation_in: crate::GraphicAnimation::Fade,
            animation_out: crate::GraphicAnimation::Fade,
            animation_ms: 1000,
            delay_ms: 0,
        }],
    };
    control
        .set_model(Some(&spec), 0, None, true, 320, 180)
        .unwrap();
    control.hide();
    let switched = control.lock().switched_at;
    let mut next = spec.clone();
    next.layers[0].opacity = 0.7;
    control
        .refresh_model(Some(&next), 0, None, true, 320, 180)
        .unwrap();
    let state = control.lock();
    assert!(state.hidden);
    assert_eq!(state.switched_at, switched);
    assert!(state.outgoing.is_none());
    assert_eq!(
        state
            .model_definition
            .as_ref()
            .unwrap()
            .model
            .as_ref()
            .unwrap()
            .spec,
        next
    );
}

#[test]
fn a_layer_logo_matches_native_pixels_and_keeps_other_layers_during_image_overrides() {
    use super::{LogoFrame, LogoOverlay};
    use crate::compositor::graphics::logo::blend_logo;
    use crate::frame::{PixelFormat, VideoFrame};
    use crate::{GraphicAnimation, GraphicContent, GraphicLayerSpec, GraphicSpec, RgbaColor};

    let mut config = logo_asset();
    config.scale = Some("80:48".to_string());
    config.position = "12:12".to_string();
    config.opacity = 0.6;
    let layer = GraphicLayerSpec {
        content: GraphicContent::Image {
            path: config.path.clone().into(),
        },
        x: 1.875,
        y: 100.0 * 12.0 / 360.0,
        width: 12.5,
        height: 100.0 * 48.0 / 360.0,
        opacity: 0.6,
        radius: 0.0,
        animation_in: GraphicAnimation::Fade,
        animation_out: GraphicAnimation::Fade,
        animation_ms: 1000,
        delay_ms: 0,
    };
    let spec = GraphicSpec {
        picture: None,
        layers: vec![layer.clone()],
    };
    let native = LogoOverlay::load(&config, 640, 360).unwrap();
    let model = LogoOverlay::from_model(&spec, 0, 640, 360).unwrap();

    for visibility in [0.0, 0.25, 0.5, 1.0] {
        let mut before = VideoFrame::new(PixelFormat::Yuv420p, 640, 360);
        let mut after = before.clone();
        blend_logo(&mut before, &native, visibility);
        model.blend_model(&mut after, &LogoFrame::default(), visibility, 1.0, false);

        for plane in 0..3 {
            assert_eq!(
                before.data(plane),
                after.data(plane),
                "plane {plane}, opacity {visibility}"
            );
        }
    }

    let mut decoration = layer;
    decoration.content = GraphicContent::Box {
        fill: RgbaColor::opaque(255, 255, 255),
    };
    decoration.x = 50.0;
    decoration.opacity = 1.0;
    let composite = GraphicSpec {
        picture: None,
        layers: vec![spec.layers[0].clone(), decoration],
    };
    let control = LogoControl::default();
    control
        .set_model(Some(&composite), 0, Some(1000), true, 640, 360)
        .unwrap();
    control.replace("replacement", &config, 640, 360).unwrap();
    let replacement = control.frame().replacement.unwrap();
    assert_eq!(
        replacement.model.as_ref().unwrap().spec.layers[1],
        composite.layers[1]
    );
    control.hide();
    control
        .set_model(Some(&composite), 0, Some(1000), true, 640, 360)
        .unwrap();
    assert!(
        control.state().hidden,
        "saving does not show a hidden model"
    );
}

#[test]
fn a_clip_logo_is_drawn_until_the_next_clip_has_none() {
    let control = LogoControl::default();

    control
        .set_clip_image(Some(&logo_asset()), 1920, 1080)
        .unwrap();
    assert!(control.frame().clip_image.is_some());
    assert!(control.has_replacement());

    // The same image is kept, not read again.
    let first = control.frame().clip_image.unwrap();
    control
        .set_clip_image(Some(&logo_asset()), 1920, 1080)
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &control.frame().clip_image.unwrap()
    ));

    control.set_clip_image(None, 1920, 1080).unwrap();
    assert!(control.frame().clip_image.is_none());
    assert!(!control.has_replacement());

    let mut missing = logo_asset();
    missing.path.push_str(".missing");
    assert!(control.set_clip_image(Some(&missing), 1920, 1080).is_err());
    assert!(control.frame().clip_image.is_none());
}

#[test]
fn hiding_the_logo_fades_it_out_and_showing_fades_it_back() {
    let mut state = LogoOverride::default();
    let start = Instant::now();

    assert_eq!(state.level(start), 1.0);

    state.set_hidden(true);
    let at = state.switched_at.unwrap();
    assert!((state.level(at + Duration::from_millis(200)) - 0.5).abs() < 0.01);
    assert_eq!(state.level(at + Duration::from_secs(1)), 0.0);

    // Shown again halfway: it fades back in from where it was.
    state.from = 0.0;
    state.switched_at = Some(at - Duration::from_secs(1));
    state.set_hidden(false);
    let back = state.switched_at.unwrap();
    assert!(state.level(back) < 0.01);
    assert_eq!(state.level(back + Duration::from_secs(1)), 1.0);
}

#[test]
fn a_cut_hides_and_shows_the_logo_at_once() {
    let mut state = LogoOverride {
        transitions: LogoTransitions {
            show: LogoTransition::Cut,
            hide: LogoTransition::Cut,
            seconds: None,
        },
        ..LogoOverride::default()
    };

    state.set_hidden(true);
    assert_eq!(state.level(state.switched_at.unwrap()), 0.0);
    state.set_hidden(false);
    assert_eq!(state.level(state.switched_at.unwrap()), 1.0);
}

#[test]
fn the_channel_sets_how_long_a_switch_takes() {
    let mut state = LogoOverride {
        transitions: LogoTransitions {
            seconds: Some(2.0),
            ..LogoTransitions::default()
        },
        ..LogoOverride::default()
    };

    state.set_hidden(true);
    let at = state.switched_at.unwrap();
    assert!((state.level(at + Duration::from_secs(1)) - 0.5).abs() < 0.01);
    assert_eq!(state.level(at + Duration::from_secs(2)), 0.0);
}

#[test]
fn a_new_image_cross_fades_with_the_one_on_air() {
    let control = LogoControl::default();
    control.set_transitions(LogoTransitions {
        seconds: Some(1.0),
        ..LogoTransitions::default()
    });

    control.replace("a.png", &logo_asset(), 1920, 1080).unwrap();
    let since = control.lock().outgoing.as_ref().unwrap().since;

    // The configured logo gives way to the new image over the second.
    let frame = control.frame_at(since + Duration::from_millis(250));
    let (image, left) = frame.outgoing.unwrap();
    assert!(image.is_none());
    assert!((left - 0.75).abs() < 0.01);
    assert!(frame.replacement.is_some());
    assert!(
        control
            .frame_at(since + Duration::from_secs(1))
            .outgoing
            .is_none()
    );

    // Back to the configured logo: the replacement fades away under it.
    control.show();
    let since = control.lock().outgoing.as_ref().unwrap().since;
    let (image, _) = control.frame_at(since).outgoing.unwrap();
    assert!(image.is_some());

    // A hidden logo has nothing on air to cross-fade from.
    let hidden = LogoControl::default();
    hidden.hide();
    hidden.lock().switched_at = None;
    hidden.replace("b.png", &logo_asset(), 1920, 1080).unwrap();
    assert!(hidden.lock().outgoing.is_none());
}

#[test]
fn a_saved_logo_goes_on_air_without_a_restart() {
    let control = LogoControl::default();
    // Nothing saved yet: the logo the playout started with stays.
    assert!(control.frame().configured.is_none());

    control
        .set_configured(Some(&logo_asset()), 1920, 1080)
        .unwrap();
    let first = control.frame().configured.unwrap().unwrap();
    assert!(control.has_replacement());
    // The first save has no earlier saved image to cross-fade from.
    assert!(control.lock().outgoing.is_none());

    // A second save cross-fades from the image on air.
    let mut moved = logo_asset();
    moved.position = "12:12".to_string();
    control.set_configured(Some(&moved), 1920, 1080).unwrap();
    let (image, _) = control.frame().outgoing.unwrap();
    assert!(std::sync::Arc::ptr_eq(&image.unwrap(), &first));
    assert!(!std::sync::Arc::ptr_eq(
        &control.frame().configured.unwrap().unwrap(),
        &first
    ));

    // Turned off: no logo, and nothing extra to draw.
    control.set_configured(None, 1920, 1080).unwrap();
    assert!(matches!(control.frame().configured, Some(None)));
    assert!(!control.has_replacement());

    // A missing image is refused and keeps what is on air.
    let mut missing = logo_asset();
    missing.path.push_str(".missing");
    assert!(control.set_configured(Some(&missing), 1920, 1080).is_err());
    assert!(matches!(control.frame().configured, Some(None)));
}

#[test]
fn a_replacement_shows_until_the_logo_is_shown_again() {
    let control = LogoControl::default();

    control
        .replace("Break/logo.png", &logo_asset(), 1920, 1080)
        .unwrap();
    assert!(control.has_replacement());
    assert_eq!(
        control.state().replacement.as_deref(),
        Some("Break/logo.png")
    );

    control.hide();
    assert!(control.state().hidden);
    // Hiding keeps the replacement for when the logo comes back.
    assert!(control.has_replacement());

    control.show();
    assert!(!control.state().hidden);
    assert!(!control.has_replacement());
    assert!(
        control
            .replace(
                "x",
                &LogoConfig {
                    path: "/missing.png".into(),
                    ..logo_asset()
                },
                1920,
                1080
            )
            .is_err()
    );
}

#[test]
fn the_logo_can_be_moved_and_put_back() {
    let control = LogoControl::default();

    assert!(control.move_to(Some("nonsense"), 1920, 1080).is_err());
    control.move_to(Some("12:H-h-12"), 1920, 1080).unwrap();
    assert_eq!(control.state().position.as_deref(), Some("12:H-h-12"));
    assert_eq!(control.frame().position.as_deref(), Some("12:H-h-12"));

    // A replacement goes to the same place, and moves with it.
    control.replace("a.png", &logo_asset(), 1920, 1080).unwrap();
    let replacement = control.frame().replacement.unwrap();
    assert_eq!(replacement.x, 12);
    assert_eq!(replacement.y, 1080 - replacement.height - 12);
    control.move_to(Some("W-w-12:12"), 1920, 1080).unwrap();
    let replacement = control.frame().replacement.unwrap();
    assert_eq!(replacement.x, 1920 - replacement.width - 12);

    control.move_to(None, 1920, 1080).unwrap();
    assert_eq!(control.state().position, None);
}

#[test]
fn a_clip_places_the_logo_unless_it_was_moved_from_outside() {
    let control = LogoControl::default();

    control
        .set_clip_position(Some("50%:100%"), 1920, 1080)
        .unwrap();
    assert_eq!(control.frame().position.as_deref(), Some("50%:100%"));
    // Not a move from outside: the state shows none.
    assert_eq!(control.state().position, None);

    control.move_to(Some("12:12"), 1920, 1080).unwrap();
    assert_eq!(control.frame().position.as_deref(), Some("12:12"));
    control.move_to(None, 1920, 1080).unwrap();
    assert_eq!(control.frame().position.as_deref(), Some("50%:100%"));

    control.set_clip_position(None, 1920, 1080).unwrap();
    assert_eq!(control.frame().position, None);
    assert!(control.set_clip_position(Some("nope"), 1920, 1080).is_err());
}

#[test]
fn logo_position_accepts_corners_and_centers() {
    assert_eq!(
        logo_position("W-w-12:12", 1920, 1080, 200, 100).unwrap(),
        (1708, 12)
    );
    assert_eq!(
        logo_position("W/2-w/2:H-h-24", 1920, 1080, 200, 100).unwrap(),
        (860, 956)
    );
    assert!(logo_position("W/0:12", 1920, 1080, 200, 100).is_err());
    // Shares of the room left around the logo.
    assert_eq!(
        logo_position("50%:100%", 1920, 1080, 200, 100).unwrap(),
        (860, 980)
    );
    assert_eq!(
        logo_position("25.5%:0%", 1920, 1080, 200, 100).unwrap(),
        (439, 0)
    );
    assert!(logo_position("120%:0%", 1920, 1080, 200, 100).is_err());
}

#[test]
fn logo_scale_accepts_output_percentages() {
    assert_eq!(
        logo_dimensions(Some("12%:-1"), 400, 200, 1920, 1080).unwrap(),
        (230, 114)
    );
    assert_eq!(
        logo_dimensions(Some("-1:10%"), 400, 200, 1920, 1080).unwrap(),
        (216, 108)
    );
}

#[test]
fn logo_scale_rejects_invalid_percentages() {
    assert!(logo_dimensions(Some("0%:-1"), 400, 200, 1920, 1080).is_err());
    assert!(logo_dimensions(Some("nan%:-1"), 400, 200, 1920, 1080).is_err());
}
#[test]
fn scheduled_logo_visibility_restores_disabled_and_hidden_states_across_refresh() {
    let control = LogoControl::default();
    let spec = crate::GraphicSpec {
        picture: None,
        layers: vec![crate::GraphicLayerSpec {
            content: crate::GraphicContent::Box {
                fill: crate::RgbaColor {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
            },
            x: 80.0,
            y: 5.0,
            width: 10.0,
            height: 10.0,
            opacity: 1.0,
            radius: 0.0,
            animation_in: crate::GraphicAnimation::Fade,
            animation_out: crate::GraphicAnimation::Fade,
            animation_ms: 400,
            delay_ms: 0,
        }],
    };
    control
        .set_model(Some(&spec), 0, None, false, 320, 180)
        .unwrap();
    assert!(!control.is_visible());
    control.set_scheduled_visible(true);
    let entered = control.lock().switched_at.unwrap();
    assert!(control.frame_at(entered).visibility < 0.01);
    assert!(
        (control
            .frame_at(entered + Duration::from_millis(200))
            .visibility
            - 0.5)
            .abs()
            < 0.01
    );
    control.set_scheduled_visible(true);
    assert!(control.is_visible());
    control
        .refresh_model(Some(&spec), 0, None, false, 320, 180)
        .unwrap();
    assert!(control.is_visible());
    // Simulate the fully entered interval without sleeping or changing a clock.
    control.lock().switched_at = None;
    control.set_scheduled_visible(false);
    let at = control.lock().switched_at.unwrap();
    assert!(!control.is_visible());
    let fading = control.frame_at(at + Duration::from_millis(200));
    assert!((fading.visibility - 0.5).abs() < 0.01);
    control.frame_at(at + Duration::from_millis(500));
    assert!(!control.state().hidden);
    control.set_scheduled_visible(true);
    control.lock().switched_at = None;
    control.set_scheduled_visible(false);
    control
        .set_model(Some(&spec), 0, None, true, 320, 180)
        .unwrap();
    control.show();
    control.frame_at(Instant::now() + Duration::from_secs(2));
    assert!(
        control.is_visible(),
        "a manual startup enable must supersede pending schedule cleanup"
    );
    control
        .set_model(Some(&spec), 0, None, true, 320, 180)
        .unwrap();
    control.hide();
    control.set_scheduled_visible(true);
    assert!(control.is_visible());
    control.set_scheduled_visible(false);
    assert!(!control.is_visible());
    assert!(control.state().hidden);
    control
        .set_model(Some(&spec), 0, None, false, 320, 180)
        .unwrap();
    control.set_scheduled_visible(true);
    control.hide();
    control.abandon_scheduled_visibility();
    assert!(!control.is_visible());
    assert!(control.state().hidden);
}
