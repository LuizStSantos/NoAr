use std::path::PathBuf;

use chrono_tz::Tz;
use quick_xml::{Reader, events::Event};

use super::*;
use crate::utils::schedule::parse_date;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ffplayout-guide-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        Self(root)
    }

    fn file(&self, relative: &str, content: &str) -> PathBuf {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();

        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn clip(path: &std::path::Path, seconds: f64) -> serde_json::Value {
    serde_json::json!({
        "source": path.to_string_lossy(),
        "in": 0.0,
        "out": seconds,
        "duration": seconds,
    })
}

/// Text of every element with the given name, in document order, after
/// checking that the whole document is well-formed.
fn texts_of(xml: &str, name: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut inside = false;
    let mut texts = vec![];

    loop {
        match reader.read_event().expect("well-formed XML") {
            Event::Start(start) if start.name().as_ref() == name.as_bytes() => {
                inside = true;
                texts.push(String::new());
            }
            Event::End(end) if end.name().as_ref() == name.as_bytes() => inside = false,
            Event::Text(text) if inside => {
                texts
                    .last_mut()
                    .unwrap()
                    .push_str(&text.xml10_content().unwrap());
            }
            Event::GeneralRef(reference) if inside => {
                let name = reference.decode().unwrap();
                let resolved = quick_xml::escape::resolve_xml_entity(&name).unwrap();
                texts.last_mut().unwrap().push_str(resolved);
            }
            Event::Eof => break,
            _ => {}
        }
    }

    texts
}

#[tokio::test]
async fn guide_merges_program_parts_absorbs_ads_and_uses_nfo_metadata() {
    let storage = TempDir::new();
    let playlists = TempDir::new();
    let timezone = Tz::America__Sao_Paulo;
    let today = time_now(&Some(timezone)).date_naive();

    storage.file(
        "Jornal/tvshow.nfo",
        "<tvshow><title>Jornal</title><plot>Notícias &amp; mais</plot>\
         <genre>Notícias</genre><mpaa>BR:L</mpaa></tvshow>",
    );
    storage.file("Jornal/poster.jpg", "");
    let block_1 = storage.file("Jornal/bloco1.mp4", "");
    let block_2 = storage.file("Jornal/bloco2.mp4", "");
    let ad = storage.file("intervalo/comercial.mp4", "");
    storage.file(
        "Novela/programa.nfo",
        "<tvshow><title>Novela</title><genre>Drama</genre></tvshow>",
    );
    storage.file("Novela/fanart.jpg", "");
    let chapter_1 = storage.file("Novela/Cap 01.mp4", "");
    storage.file(
        "Novela/Cap 01.nfo",
        "<episodedetails><title>O começo</title><season>1</season><episode>1</episode>\
         <plot>Primeiro capítulo.</plot><genre>Romance</genre></episodedetails>",
    );
    storage.file("Novela/Cap 01-thumb.jpg", "");
    let chapter_2 = storage.file("Novela/Cap 02.mp4", "");
    let loose = storage.file("avulso_sem_nfo.mp4", "");

    let mut special = clip(&loose, 300.0);
    special["title"] = "Clipe especial".into();
    let mut commercial = clip(&ad, 60.0);
    commercial["ad"] = true.into();
    let playlist = serde_json::json!({
        "date": today.format("%Y-%m-%d").to_string(),
        "program": [
            clip(&block_1, 600.0),
            commercial,
            clip(&block_2, 600.0),
            clip(&chapter_1, 1800.0),
            clip(&chapter_2, 1800.0),
            special,
        ]
    });
    playlists.file(
        &today.format("%Y/%m/%Y-%m-%d.json").to_string(),
        &playlist.to_string(),
    );

    let mut config = PlayoutConfig::default();
    config.channel.storage = storage.0.clone();
    config.channel.playlists = playlists.0.clone();
    config.channel.timezone = Some(timezone);
    config.playlist.start_sec = Some(0.0);
    let channel = XmltvChannel {
        id: "canal.br".to_string(),
        name: "Canal & Cia".to_string(),
    };
    let options = XmltvOptions {
        days: 1,
        lang: Some("pt".to_string()),
        image_base_url: "http://tv.local/xmltv/1/images".to_string(),
    };

    let xml = build_xmltv(&config, &channel, &[], &options).await.unwrap();
    let day = today.format("%Y%m%d").to_string();

    assert_eq!(texts_of(&xml, "display-name"), ["Canal & Cia"]);
    assert_eq!(
        texts_of(&xml, "title"),
        ["Jornal", "Novela", "Novela", "Clipe especial"]
    );
    assert_eq!(texts_of(&xml, "sub-title"), ["O começo"]);
    assert_eq!(
        texts_of(&xml, "desc"),
        ["Notícias & mais", "Primeiro capítulo."]
    );
    assert_eq!(
        texts_of(&xml, "category"),
        ["Notícias", "Drama", "Romance", "Drama"]
    );
    assert_eq!(texts_of(&xml, "episode-num"), ["0.0.", "S01E01"]);
    assert_eq!(texts_of(&xml, "value"), ["L"]);
    // The ad between the two news blocks is part of the news programme.
    assert!(xml.contains(&format!(
        "<programme start=\"{day}000000 -0300\" stop=\"{day}002100 -0300\" channel=\"canal.br\">"
    )));
    assert!(xml.contains(&format!(
        "<programme start=\"{day}002100 -0300\" stop=\"{day}005100 -0300\""
    )));
    assert!(xml.contains("<title lang=\"pt\">Jornal</title>"));
    assert!(xml.contains("<icon src=\"http://tv.local/xmltv/1/images/Jornal/poster.jpg\"/>"));
    assert!(
        xml.contains("<icon src=\"http://tv.local/xmltv/1/images/Novela/Cap%2001-thumb.jpg\"/>")
    );
    assert!(xml.contains(
        "<image type=\"backdrop\" orient=\"L\">http://tv.local/xmltv/1/images/Novela/fanart.jpg</image>"
    ));
    assert_eq!(xml.matches("<programme ").count(), 4);
}

#[tokio::test]
async fn clip_programmes_group_like_the_guide() {
    let storage = TempDir::new();

    storage.file(
        "Jornal/tvshow.nfo",
        "<tvshow><title>Jornal</title></tvshow>",
    );
    let block_1 = storage.file("Jornal/bloco1.mp4", "");
    let block_2 = storage.file("Jornal/bloco2.mp4", "");
    let ad = storage.file("intervalo/comercial.mp4", "");
    let filler = storage.file("filler/vinheta.mp4", "");
    let loose = storage.file("avulso.mp4", "");

    let mut config = PlayoutConfig::default();
    config.channel.storage = storage.0.clone();
    config.storage.filler = "filler".to_string();
    config.storage.filler_path = storage.0.join("filler");

    let query = |path: &std::path::Path, title: Option<&str>, ad: bool| ClipQuery {
        source: path.to_string_lossy().to_string(),
        title: title.map(str::to_string),
        ad,
    };
    let programmes = clip_programmes(
        &config,
        vec![
            query(&block_1, None, false),
            query(&ad, None, true),
            query(&block_2, None, false),
            query(&filler, None, false),
            query(&loose, Some(" Especial "), false),
        ],
    )
    .await;

    let titles: Vec<&str> = programmes.iter().map(|p| p.title.as_str()).collect();
    let breaks: Vec<bool> = programmes.iter().map(|p| p.is_break).collect();

    assert_eq!(titles[0], "Jornal");
    assert_eq!(titles[2], "Jornal");
    assert_eq!(titles[4], "Especial");
    assert_eq!(breaks, [false, true, false, true, false]);
    assert_eq!(programmes[0].key, programmes[2].key);
    assert_ne!(programmes[0].key, programmes[4].key);
}

#[tokio::test]
async fn guide_requires_an_initialized_playlist_start() {
    let config = PlayoutConfig::default();
    let channel = XmltvChannel {
        id: "x".to_string(),
        name: "x".to_string(),
    };
    let options = XmltvOptions {
        days: 1,
        lang: None,
        image_base_url: String::new(),
    };

    assert!(matches!(
        build_xmltv(&config, &channel, &[], &options).await,
        Err(ServiceError::Conflict(_))
    ));
}

#[test]
fn next_playlist_cuts_the_previous_one() {
    let at = |time: &str| NaiveDateTime::parse_from_str(time, "%F %T").unwrap();
    let slot = |start: &str, stop: &str| playlist_slot(at(start), at(stop), "/m/a.mp4");
    let mut slots = vec![
        slot("2026-10-14 05:00:00", "2026-10-14 05:50:00"),
        slot("2026-10-14 05:50:00", "2026-10-14 06:20:00"),
        slot("2026-10-14 06:20:00", "2026-10-14 06:40:00"),
    ];

    cut_at(&mut slots, at("2026-10-14 06:00:00"));

    assert_eq!(slots.len(), 2);
    assert_eq!(slots[1].stop, at("2026-10-14 06:00:00"));
}

#[test]
fn formats_episode_numbers_dates_and_ratings() {
    let nfo = |season, episode| Nfo {
        season,
        episode,
        ..Nfo::default()
    };
    let dated = Nfo {
        premiered: Some("2019-03-04".to_string()),
        ..Nfo::default()
    };
    let year_only = Nfo {
        year: Some("1999".to_string()),
        ..Nfo::default()
    };

    assert_eq!(
        episode_numbers(&nfo(Some(2), Some(5))),
        Some(("1.4.".to_string(), "S02E05".to_string()))
    );
    assert_eq!(
        episode_numbers(&nfo(None, Some(12))),
        Some((".11.".to_string(), "E12".to_string()))
    );
    assert_eq!(episode_numbers(&nfo(None, None)), None);
    assert_eq!(xmltv_date(&dated).as_deref(), Some("20190304"));
    assert_eq!(xmltv_date(&year_only).as_deref(), Some("1999"));
    assert_eq!(
        ratings("US:PG-13 / BR:12"),
        [(Some("US"), "PG-13"), (Some("BR"), "12")]
    );
    assert_eq!(ratings("Livre"), [(None, "Livre")]);
}

#[test]
fn builds_safe_image_urls_and_text() {
    let images = ImageUrls {
        base: "https://tv.example/xmltv/1/images",
        storage: PathBuf::from("/media"),
    };

    assert_eq!(
        images.url(&Image::Local(PathBuf::from(
            "/media/Programação/capa #1.jpg"
        ))),
        Some("https://tv.example/xmltv/1/images/Programa%C3%A7%C3%A3o/capa%20%231.jpg".to_string())
    );
    assert_eq!(
        images.url(&Image::Local(PathBuf::from("/other/a.jpg"))),
        None
    );
    assert_eq!(
        images.url(&Image::Remote("https://cdn/x.jpg".to_string())),
        Some("https://cdn/x.jpg".to_string())
    );
    assert_eq!(xml_safe("a\u{0B}b\tc"), "ab\tc");
    assert_eq!(title_from_file_name("/m/meu_clipe.mp4"), "meu clipe");
}

fn playlist_slot(start: NaiveDateTime, stop: NaiveDateTime, source: &str) -> Slot {
    Slot {
        start,
        stop,
        day_end: NaiveDateTime::parse_from_str("2026-10-15 06:00:00", "%F %T").unwrap(),
        source: source.to_string(),
        title: None,
        ad: false,
        scheduled: false,
    }
}

fn scheduled(kind: ScheduleKind, entry: EntryMode, after: AfterMode) -> Occurrence {
    let event = ScheduleEvent {
        id: 1,
        kind,
        title: "Event".to_string(),
        sources: vec!["Breaks/a.mp4".to_string()],
        duration: 210.0,
        start_date: "2026-10-14".to_string(),
        start_time: "12:10:00".to_string(),
        end_time: Some("12:20:00".to_string()),
        entry_mode: entry,
        after_mode: after,
        show_in_guide: kind == ScheduleKind::Live,
        enabled: true,
        ..ScheduleEvent::default()
    };

    event
        .occurrence_on(parse_date("2026-10-14").unwrap())
        .unwrap()
}

fn timeline(slots: &[Slot]) -> Vec<(String, String, String)> {
    slots
        .iter()
        .map(|slot| {
            (
                slot.start.format("%H:%M:%S").to_string(),
                slot.stop.format("%H:%M:%S").to_string(),
                slot.source.clone(),
            )
        })
        .collect()
}

fn three_clips() -> Vec<Slot> {
    let at =
        |time: &str| NaiveDateTime::parse_from_str(&format!("2026-10-14 {time}"), "%F %T").unwrap();

    vec![
        playlist_slot(at("12:00:00"), at("12:30:00"), "a"),
        playlist_slot(at("12:30:00"), at("13:00:00"), "b"),
        playlist_slot(at("13:00:00"), at("13:30:00"), "c"),
    ]
}

fn row(start: &str, stop: &str, source: &str) -> (String, String, String) {
    (start.to_string(), stop.to_string(), source.to_string())
}

#[test]
fn waiting_events_start_after_the_clip_and_delay_the_rest() {
    let occurrence = scheduled(
        ScheduleKind::Media,
        EntryMode::AfterClip,
        AfterMode::Continue,
    );

    let slots = apply_schedule(three_clips(), &[occurrence]);

    assert_eq!(
        timeline(&slots),
        [
            row("12:00:00", "12:30:00", "a"),
            row("12:30:00", "12:33:30", "Breaks/a.mp4"),
            row("12:33:30", "13:03:30", "b"),
            row("13:03:30", "13:33:30", "c"),
        ]
    );
    assert!(slots[1].ad, "a hidden event is absorbed like an ad");
}

#[test]
fn interrupting_events_split_the_clip_and_resume_it() {
    let occurrence = scheduled(
        ScheduleKind::Media,
        EntryMode::Interrupt,
        AfterMode::Continue,
    );

    let slots = apply_schedule(three_clips(), &[occurrence]);

    assert_eq!(
        timeline(&slots),
        [
            row("12:00:00", "12:10:00", "a"),
            row("12:10:00", "12:13:30", "Breaks/a.mp4"),
            row("12:13:30", "12:33:30", "a"),
            row("12:33:30", "13:03:30", "b"),
            row("13:03:30", "13:33:30", "c"),
        ]
    );
}

#[test]
fn lives_on_the_clock_cover_the_programme() {
    let occurrence = scheduled(ScheduleKind::Live, EntryMode::Interrupt, AfterMode::Clock);

    let slots = apply_schedule(three_clips(), &[occurrence]);

    assert_eq!(
        timeline(&slots),
        [
            row("12:00:00", "12:10:00", "a"),
            row("12:10:00", "12:20:00", "Breaks/a.mp4"),
            row("12:20:00", "12:30:00", "a"),
            row("12:30:00", "13:00:00", "b"),
            row("13:00:00", "13:30:00", "c"),
        ]
    );
    assert!(!slots[1].ad, "a live shown in the guide is its own entry");
}

#[test]
fn delays_stop_at_the_end_of_the_playlist_day() {
    let at = |time: &str| NaiveDateTime::parse_from_str(time, "%F %T").unwrap();
    let mut last = playlist_slot(at("2026-10-15 05:50:00"), at("2026-10-15 06:00:00"), "late");
    last.day_end = at("2026-10-15 06:00:00");

    let slots = delay(
        vec![last],
        at("2026-10-15 05:00:00"),
        TimeDelta::try_minutes(5).unwrap(),
        at("2026-10-15 06:00:00"),
    );

    assert_eq!(slots[0].start, at("2026-10-15 05:55:00"));
    assert_eq!(slots[0].stop, at("2026-10-15 06:00:00"));
}
