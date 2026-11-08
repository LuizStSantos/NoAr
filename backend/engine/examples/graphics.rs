//! Isolated HLS evidence for permanent graphics before the application migration.
//! Run with synthetic media only; arguments are INPUT, OUTPUT_DIRECTORY, LOGO_PNG.

use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use noar_engine::{
    AlwaysOnSlot, BlockAlign, ClipResult, GraphicAnimation, GraphicContent, GraphicLayerSpec,
    GraphicSpec, GraphicsControl, LogoConfig, LogoFade, OutputConfig, Playout, RgbaColor,
    render_graphic,
};

fn label(text: &str, x: f64, y: f64, width: f64, height: f64) -> GraphicSpec {
    GraphicSpec {
        picture: None,
        layers: vec![GraphicLayerSpec {
            content: GraphicContent::Text {
                text: text.to_string(),
                scroll: None,
                font_family: None,
                font_size: 4.0,
                bold: true,
                color: RgbaColor::opaque(20, 20, 20),
                align: BlockAlign::Left,
                fill: Some(RgbaColor::opaque(255, 255, 255)),
            },
            x,
            y,
            width,
            height,
            opacity: 1.0,
            radius: 0.0,
            animation_in: GraphicAnimation::None,
            animation_out: GraphicAnimation::None,
            animation_ms: 0,
            delay_ms: 0,
        }],
    }
}

fn play_stage(playout: &mut Playout, input: &str, name: &str) -> Result<()> {
    eprintln!("Stage: {name}");
    let result =
        playout.play_with_timing_and_logo_fade(input, Some(0.0), Some(8.0), LogoFade::default())?;
    ensure!(
        result == ClipResult::Played,
        "stage {name} did not play its source: {result:?}"
    );
    Ok(())
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let arguments: Vec<_> = env::args().skip(1).collect();
    ensure!(
        arguments.len() == 3,
        "usage: graphics INPUT OUTPUT_DIRECTORY LOGO_PNG"
    );
    let input = &arguments[0];
    let directory = PathBuf::from(&arguments[1]);
    fs::create_dir_all(&directory)?;
    let control = GraphicsControl::default();
    let mut config = OutputConfig::new(640, 360, 25, 48_000).with_graphics_control(control.clone());

    // This is the unchanged configured-logo path. The permanent slots are
    // independent infrastructure and must not erase or draw this logo twice.
    config.logo = Some(LogoConfig {
        path: arguments[2].clone(),
        scale: Some("64:36".to_string()),
        opacity: 1.0,
        position: "550:12".to_string(),
    });
    let playlist = directory.join("permanent.m3u8");
    let mut playout = Playout::open_hls(
        playlist.to_str().context("non-UTF-8 output directory")?,
        config,
        5.0,
        &[],
        None,
        2,
        64,
    )?;

    play_stage(&mut playout, input, "legacy-only")?;
    control.set_always_on(
        AlwaysOnSlot::Logo,
        render_graphic(&label("ALWAYS LOGO", 5.0, 5.0, 32.0, 10.0), 640, 360)?,
        true,
    )?;
    control.set_always_on(
        AlwaysOnSlot::Ticker,
        render_graphic(&label("ALWAYS ON TEXT", 5.0, 85.0, 90.0, 10.0), 640, 360)?,
        true,
    )?;
    play_stage(&mut playout, input, "permanent-roles")?;

    control.take(
        render_graphic(&label("TEMPORARY GC", 25.0, 60.0, 50.0, 10.0), 640, 360)?,
        None,
    );
    play_stage(&mut playout, input, "temporary-gc")?;
    control.take_all_out();
    play_stage(&mut playout, input, "take-all-out-keeps-permanent")?;
    control.set_always_on_visible(AlwaysOnSlot::Logo, false);
    play_stage(&mut playout, input, "logo-hidden-ticker-stays")?;
    control.set_always_on_visible(AlwaysOnSlot::Logo, true);
    play_stage(&mut playout, input, "logo-restored")?;
    playout.finish()?;
    Ok(())
}
