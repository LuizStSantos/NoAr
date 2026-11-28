//! The font system, its bounded glyph cache and the uploaded fonts.

use std::{
    collections::BTreeSet,
    mem,
    sync::{Mutex, OnceLock, PoisonError},
};

use anyhow::{Context, Result, anyhow};
use cosmic_text::{Attrs, Family, FontSystem, SwashCache, Weight};

static TEXT_RENDERER: OnceLock<Mutex<TextRenderer>> = OnceLock::new();

const MAX_GLYPH_CACHE_ENTRIES: usize = 4096;
const MAX_GLYPH_CACHE_BYTES: usize = 32 * 1024 * 1024;

// Swash caches are append-only while drawing. Unchanged entry counts need no
// repeated byte scan. Evict the largest entries first with 25% headroom, keeping
// small reusable glyphs instead of forcing a complete cache rebuild.
pub(super) fn trim_glyph_cache(cache: &mut SwashCache, checked: &mut Option<(usize, usize)>) {
    let counts = (cache.image_cache.len(), cache.outline_command_cache.len());

    if *checked == Some(counts) {
        return;
    }

    let mut entries = Vec::with_capacity(counts.0 + counts.1);

    for (key, image) in &cache.image_cache {
        entries.push((
            *key,
            true,
            image.as_ref().map_or(0, |image| image.data.capacity()),
        ));
    }

    for (key, commands) in &cache.outline_command_cache {
        entries.push((
            *key,
            false,
            commands
                .as_ref()
                .map_or(0, |commands| mem::size_of_val(commands.as_ref())),
        ));
    }

    let mut bytes = entries
        .iter()
        .fold(0usize, |sum, entry| sum.saturating_add(entry.2));
    let mut count = entries.len();

    if count > MAX_GLYPH_CACHE_ENTRIES || bytes > MAX_GLYPH_CACHE_BYTES {
        entries.sort_unstable_by_key(|entry| std::cmp::Reverse(entry.2));

        for (key, image, size) in entries {
            if count <= MAX_GLYPH_CACHE_ENTRIES * 3 / 4 && bytes <= MAX_GLYPH_CACHE_BYTES * 3 / 4 {
                break;
            }

            if image {
                cache.image_cache.remove(&key);
            } else {
                cache.outline_command_cache.remove(&key);
            }

            count -= 1;
            bytes = bytes.saturating_sub(size);
        }
        cache.image_cache.shrink_to_fit();
        cache.outline_command_cache.shrink_to_fit();
    }
    *checked = Some((cache.image_cache.len(), cache.outline_command_cache.len()));
}

pub(super) struct TextRenderer {
    pub(super) font_system: FontSystem,
    pub(super) swash_cache: SwashCache,
    pub(super) cache_checked: Option<(usize, usize)>,
}

impl TextRenderer {
    fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            cache_checked: None,
        }
    }
}

pub(crate) fn init() {
    TEXT_RENDERER.get_or_init(|| Mutex::new(TextRenderer::new()));
}

pub(super) fn renderer() -> &'static Mutex<TextRenderer> {
    TEXT_RENDERER.get_or_init(|| Mutex::new(TextRenderer::new()))
}

/// Validate one immutable OpenType/TrueType face before persisting an upload.
pub(crate) fn uploaded_font_name(data: &[u8]) -> Result<String> {
    if !matches!(data.get(..4), Some(b"OTTO" | b"\0\x01\0\0")) {
        return Err(anyhow!("Upload a TrueType (.ttf) or OpenType (.otf) font"));
    }

    let mut database = cosmic_text::fontdb::Database::new();
    database.load_font_data(data.to_vec());
    let faces: Vec<_> = database.faces().collect();

    if faces.len() != 1 {
        return Err(anyhow!("The file must contain exactly one valid font face"));
    }

    faces[0]
        .families
        .first()
        .map(|(name, _)| name.clone())
        .context("The font has no family name")
}

/// Namespace faces by asset identity so identical filenames cannot replace a model's font.
pub(crate) fn register_uploaded_font(family: &str, data: Vec<u8>) -> Result<()> {
    uploaded_font_name(&data)?;
    let mut renderer = renderer().lock().unwrap_or_else(PoisonError::into_inner);
    let database = renderer.font_system.db_mut();

    if database
        .faces()
        .any(|face| face.families.iter().any(|(name, _)| name == family))
    {
        return Ok(());
    }

    let ids = database.load_font_source(cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(
        data,
    )));

    for id in ids {
        let mut face = database
            .face(id)
            .context("The validated font could not be registered")?
            .clone();
        database.remove_face(id);
        face.families
            .iter_mut()
            .for_each(|(name, _)| *name = family.to_string());
        database.push_face_info(face);
    }

    Ok(())
}

pub(crate) fn available_font_families() -> Vec<String> {
    let renderer = renderer().lock().unwrap_or_else(PoisonError::into_inner);
    renderer
        .font_system
        .db()
        .faces()
        .filter_map(|face| face.families.first().map(|(family, _)| family.clone()))
        .filter(|family| !family.starts_with("NoArFont_"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The attributes that select `family` at `weight`: the requested weight and
/// an upright style when the family has such a face, otherwise the weight and
/// style of its nearest face. An uploaded font often has a single face (only
/// bold, or bold italic); asking it for another weight made the shaper fall
/// back to the default font.
pub(super) fn family_attrs<'a>(
    font_system: &FontSystem,
    family: &'a str,
    weight: Weight,
) -> Attrs<'a> {
    let faces: Vec<_> = font_system
        .db()
        .faces()
        .filter(|face| face.families.iter().any(|(name, _)| name == family))
        .collect();
    let attrs = Attrs::new().family(Family::Name(family)).weight(weight);

    if faces.iter().any(|face| face.weight == weight) {
        let upright = faces
            .iter()
            .any(|face| face.weight == weight && face.style == cosmic_text::Style::Normal);

        return if upright {
            attrs
        } else {
            let style = faces
                .iter()
                .find(|face| face.weight == weight)
                .map_or(cosmic_text::Style::Normal, |face| face.style);

            attrs.style(style)
        };
    }

    match faces
        .iter()
        .min_by_key(|face| (i32::from(face.weight.0) - i32::from(weight.0)).abs())
    {
        Some(face) => attrs.weight(face.weight).style(face.style),
        None => attrs,
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use crate::compositor::text::overlay::render_text_overlay;
    use crate::utils::config::TextConfig;

    fn key(glyph: u16) -> cosmic_text::CacheKey {
        cosmic_text::CacheKey::new(
            Default::default(),
            glyph,
            24.0,
            (0.0, 0.0),
            Weight::NORMAL,
            cosmic_text::CacheKeyFlags::empty(),
        )
        .0
    }

    #[test]
    fn retains_small_cache_but_releases_excess_entries() {
        let mut cache = SwashCache::new();
        cache.image_cache.insert(key(0), None);
        trim_glyph_cache(&mut cache, &mut None);
        assert_eq!(cache.image_cache.len(), 1);

        for glyph in 1..=MAX_GLYPH_CACHE_ENTRIES {
            cache.image_cache.insert(key(glyph as u16), None);
        }
        trim_glyph_cache(&mut cache, &mut None);
        assert_eq!(cache.image_cache.len(), MAX_GLYPH_CACHE_ENTRIES * 3 / 4);
        assert!(cache.outline_command_cache.is_empty());
    }

    #[test]
    fn releases_large_raster_even_with_few_entries() {
        let mut cache = SwashCache::new();
        cache.image_cache.insert(key(1), None);
        let mut image = cosmic_text::SwashImage::new();
        image.data.resize(MAX_GLYPH_CACHE_BYTES + 1, 0);
        cache.image_cache.insert(key(0), Some(image));
        trim_glyph_cache(&mut cache, &mut None);
        assert!(!cache.image_cache.contains_key(&key(0)));
        assert!(cache.image_cache.contains_key(&key(1)));
    }

    #[test]
    fn repeated_growth_retains_headroom_and_reuses_unchanged_cache() {
        let mut cache = SwashCache::new();
        let mut checked = None;

        for cycle in 0..20 {
            for glyph in 0..5000 {
                cache.image_cache.insert(key(glyph), None);
            }
            trim_glyph_cache(&mut cache, &mut checked);
            assert_eq!(
                cache.image_cache.len(),
                MAX_GLYPH_CACHE_ENTRIES * 3 / 4,
                "cycle {cycle}"
            );
            let keys: BTreeSet<_> = cache.image_cache.keys().copied().collect();
            trim_glyph_cache(&mut cache, &mut checked);
            assert_eq!(keys, cache.image_cache.keys().copied().collect());
        }
    }

    #[test]
    #[ignore = "manual render-time and cache-growth stress test"]
    fn changing_text_render_stress() {
        let started = std::time::Instant::now();
        let mut worst = std::time::Duration::ZERO;

        for index in 0..1000 {
            let config = TextConfig {
                font_size: (20 + index % 80) as f32,
                ..TextConfig::default()
            };
            let text = format!("Subtitle {index}: changing sizes — ÄÖÜ é 中文 العربية");
            let draw = std::time::Instant::now();
            render_text_overlay(&config, &text, 1920, 1080).unwrap();
            worst = worst.max(draw.elapsed());
        }
        eprintln!(
            "1000 changing overlays: {:?}, slowest render: {:?}",
            started.elapsed(),
            worst
        );
    }
}

#[cfg(test)]
mod uploaded_font_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn validated_upload_selects_its_exact_bytes_by_immutable_family() {
        let data = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests_assets/fonts/graphics.ttf"),
        )
        .unwrap();
        assert!(uploaded_font_name(b"not a font").is_err());
        assert!(uploaded_font_name(&data).unwrap().contains("NoAr"));
        let alias = "NoArFont_stage3_verified";
        register_uploaded_font(alias, data.clone()).unwrap();
        register_uploaded_font(alias, data.clone()).unwrap();
        let renderer = renderer().lock().unwrap_or_else(PoisonError::into_inner);
        let database = renderer.font_system.db();
        let families = [Family::Name(alias)];
        let id = database
            .query(&cosmic_text::fontdb::Query {
                families: &families,
                ..Default::default()
            })
            .unwrap();
        assert!(
            database
                .with_face_data(id, |selected, _| selected == data)
                .unwrap()
        );
        assert_eq!(
            database
                .faces()
                .filter(|face| face.families.iter().any(|(name, _)| name == alias))
                .count(),
            1
        );
    }

    #[test]
    fn a_single_face_upload_is_used_whatever_weight_is_asked() {
        let data = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests_assets/fonts/graphics.ttf"),
        )
        .unwrap();
        let alias = "NoArFont_single_face_weight";
        register_uploaded_font(alias, data).unwrap();
        let renderer = renderer().lock().unwrap_or_else(PoisonError::into_inner);
        let face_weight = renderer
            .font_system
            .db()
            .faces()
            .find(|face| face.families.iter().any(|(name, _)| name == alias))
            .unwrap()
            .weight;
        let other = if face_weight == Weight::BOLD {
            Weight::NORMAL
        } else {
            Weight::BOLD
        };

        for asked in [face_weight, other] {
            let attrs = family_attrs(&renderer.font_system, alias, asked);
            let query = cosmic_text::fontdb::Query {
                families: &[Family::Name(alias)],
                weight: attrs.weight,
                style: attrs.style,
                ..Default::default()
            };
            let id = renderer.font_system.db().query(&query);

            assert!(
                id.is_some(),
                "the uploaded face is selected when {asked:?} is asked"
            );
            assert_eq!(attrs.weight, face_weight);
        }

        assert_eq!(
            family_attrs(&renderer.font_system, "No such family", Weight::BOLD).weight,
            Weight::BOLD
        );
    }
}
