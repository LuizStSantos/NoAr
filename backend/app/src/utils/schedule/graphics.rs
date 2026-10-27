//! Graphics at a set time: what a quick scene does to the logo and the
//! L-shape, applied when the event starts and undone when it ends, once or
//! on chosen weekdays. They change the picture on air, never the programme.

use chrono::{Datelike, NaiveDate, NaiveDateTime, TimeDelta};
use serde::Deserialize;

use super::{parse_date, parse_time};
use crate::{
    db::models::GraphicsEvent,
    file::norm_abs_path,
    utils::{config::GraphicsScene, errors::ServiceError},
};

/// A graphics event as sent by the web interface.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphicsEventInput {
    #[serde(default)]
    pub title: String,
    pub actions: GraphicsScene,
    pub start_date: String,
    pub start_time: String,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default)]
    pub repeat_days: u8,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Check an event from the web interface and turn it into a row: its
/// images become storage-relative.
pub fn prepare_graphics_event(
    storage: &std::path::Path,
    channel_id: i32,
    id: i32,
    input: GraphicsEventInput,
) -> Result<GraphicsEvent, ServiceError> {
    let bad_request = ServiceError::BadRequest;
    let start_date = parse_date(&input.start_date).map_err(bad_request)?;
    let start_time = parse_time(&input.start_time).map_err(bad_request)?;
    let end_time = input
        .end_time
        .as_deref()
        .map(str::trim)
        .filter(|end| !end.is_empty())
        .map(|end| parse_time(end).map_err(bad_request))
        .transpose()?;

    if input.repeat_days > 127 {
        return Err(bad_request("repeat_days must be between 0 and 127".into()));
    }

    let title = input.title.trim().to_string();
    let mut actions = GraphicsScene {
        name: title.clone(),
        ..input.actions
    };

    actions.check().map_err(bad_request)?;

    for image in [&mut actions.logo_image, &mut actions.lshape_graphic] {
        *image = match image
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            Some(path) => Some(norm_abs_path(storage, path)?.2),
            None => None,
        };
    }

    Ok(GraphicsEvent {
        id,
        channel_id,
        title,
        actions,
        start_date: start_date.format("%Y-%m-%d").to_string(),
        start_time: start_time.format("%H:%M:%S").to_string(),
        end_time: end_time.map(|end| end.format("%H:%M:%S").to_string()),
        repeat_days: input.repeat_days,
        enabled: input.enabled,
    })
}

impl GraphicsEvent {
    fn occurs_on(&self, date: NaiveDate) -> bool {
        let Ok(first) = parse_date(&self.start_date) else {
            return false;
        };

        if self.repeat_days == 0 {
            date == first
        } else {
            date >= first && self.repeat_days & (1 << date.weekday().num_days_from_sunday()) != 0
        }
    }

    /// When it starts on `date`, and ends (an end before the start is the
    /// next day), if it is enabled and happens that day.
    pub fn window_on(&self, date: NaiveDate) -> Option<(NaiveDateTime, Option<NaiveDateTime>)> {
        if !self.enabled || !self.occurs_on(date) {
            return None;
        }

        let start = date.and_time(parse_time(&self.start_time).ok()?);
        let end = match self.end_time.as_deref() {
            Some(end) => {
                let mut end = date.and_time(parse_time(end).ok()?);

                if end <= start {
                    end += TimeDelta::try_days(1)?;
                }

                Some(end)
            }
            None => None,
        };

        Some((start, end))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::utils::config::{SceneLShape, SceneLogo};

    fn input(start: &str, end: Option<&str>, repeat_days: u8) -> GraphicsEventInput {
        GraphicsEventInput {
            title: " Patrocínio ".to_string(),
            actions: GraphicsScene {
                logo: SceneLogo::Hide,
                lshape: SceneLShape::On,
                lshape_corner: Some("bottom-left".to_string()),
                lshape_scale: Some(70.0),
                ..GraphicsScene::default()
            },
            start_date: "2026-10-27".to_string(),
            start_time: start.to_string(),
            end_time: end.map(str::to_string),
            repeat_days,
            enabled: true,
        }
    }

    #[test]
    fn a_graphics_event_is_checked_and_placed_in_time() {
        let storage = Path::new("/tmp");
        let event =
            prepare_graphics_event(storage, 1, 0, input("12:00", Some("12:05"), 0)).unwrap();

        assert_eq!(event.title, "Patrocínio");
        assert_eq!(event.start_time, "12:00:00");
        let day = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        let (start, end) = event.window_on(day).unwrap();
        assert_eq!(start, day.and_hms_opt(12, 0, 0).unwrap());
        assert_eq!(end, Some(day.and_hms_opt(12, 5, 0).unwrap()));
        assert!(event.window_on(day.succ_opt().unwrap()).is_none());

        // Past midnight it ends the next day; weekdays repeat it.
        let late =
            prepare_graphics_event(storage, 1, 0, input("23:58", Some("00:02"), 127)).unwrap();
        let next = day.succ_opt().unwrap();
        assert_eq!(
            late.window_on(next).unwrap().1,
            Some(next.succ_opt().unwrap().and_hms_opt(0, 2, 0).unwrap())
        );

        let mut wrong = input("12:00", None, 0);
        wrong.actions.lshape_corner = Some("middle".to_string());
        assert!(prepare_graphics_event(storage, 1, 0, wrong).is_err());
        assert!(prepare_graphics_event(storage, 1, 0, input("25:00", None, 0)).is_err());
    }
}
