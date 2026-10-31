//! Persisted user itinerary state. This is semantic trip data shared by desktop
//! and HTTP clients; presentation and map layout remain client-owned.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{TravelDateRange, normalize_city_query};

const MAX_TRIP_DAYS: u8 = 30;
const MAX_PLAN_ITEMS: usize = 500;
const MAX_CUSTOM_PLACES: usize = 100;
const MAX_KEY_CHARS: usize = 256;
const MAX_TITLE_CHARS: usize = 120;
const MAX_PLACE_NAME_CHARS: usize = 200;
const MAX_NOTE_CHARS: usize = 4_000;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct TravelPlanContext {
    pub city: String,
    pub days: u8,
    pub date_range: Option<TravelDateRange>,
}

impl TravelPlanContext {
    pub fn validate(mut self) -> Result<Self, TravelPlanError> {
        self.city = normalize_city_query(&self.city);
        if self.city.is_empty() || self.city.chars().count() > 120 {
            return Err(TravelPlanError::InvalidCity);
        }
        if !(1..=MAX_TRIP_DAYS).contains(&self.days) {
            return Err(TravelPlanError::InvalidDays);
        }
        if let Some(range) = &self.date_range
            && (!is_iso_date(&range.start) || !is_iso_date(&range.end) || range.start > range.end)
        {
            return Err(TravelPlanError::InvalidDateRange);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct TravelPlanPlace {
    pub name: String,
    pub note: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct TravelPlan {
    pub context: TravelPlanContext,
    pub title: String,
    pub departure: String,
    pub notes: String,
    /// User-entered stops keyed by stable client-generated place keys.
    pub custom_places: BTreeMap<String, TravelPlanPlace>,
    /// Ordered place keys for the itinerary.
    pub selected_keys: Vec<String>,
    pub assigned_days: BTreeMap<String, u8>,
    pub must_visit_keys: Vec<String>,
    /// Compare-and-swap token assigned and advanced by the persistence layer.
    pub revision: i64,
    /// Unix timestamp maintained by the backend; clients' values are ignored on save.
    pub updated_at: i64,
}

impl TravelPlan {
    /// Validate and normalize a client write before it reaches persistence.
    pub fn validate(mut self) -> Result<Self, TravelPlanError> {
        self.context = self.context.validate()?;
        self.title = normalize_text(self.title, MAX_TITLE_CHARS, false)?;
        self.departure = normalize_text(self.departure, MAX_TITLE_CHARS, false)?;
        self.notes = normalize_text(self.notes, MAX_NOTE_CHARS, true)?;
        if self.custom_places.len() > MAX_CUSTOM_PLACES {
            return Err(TravelPlanError::TooManyCustomPlaces);
        }
        for (key, place) in &mut self.custom_places {
            if !is_valid_key(key) || !key.starts_with("custom:") {
                return Err(TravelPlanError::InvalidItemKey);
            }
            place.name = normalize_text(place.name.clone(), MAX_PLACE_NAME_CHARS, false)?;
            if place.name.is_empty() {
                return Err(TravelPlanError::InvalidPlaceName);
            }
            place.note = normalize_text(place.note.clone(), MAX_NOTE_CHARS, true)?;
        }
        self.selected_keys = normalize_keys(self.selected_keys)?;
        self.must_visit_keys = normalize_keys(self.must_visit_keys)?;

        if self.revision < 0 || self.revision == i64::MAX {
            return Err(TravelPlanError::InvalidRevision);
        }
        if self.assigned_days.len() > MAX_PLAN_ITEMS
            || self
                .assigned_days
                .iter()
                .any(|(key, day)| !is_valid_key(key) || !(1..=self.context.days).contains(day))
        {
            return Err(TravelPlanError::InvalidAssignment);
        }
        // Prevent clients from forging the server-owned persistence timestamp.
        self.updated_at = 0;
        Ok(self)
    }
}

fn normalize_text(
    value: String,
    max_chars: usize,
    allow_newlines: bool,
) -> Result<String, TravelPlanError> {
    let value = value.trim().to_string();
    if value.chars().count() > max_chars
        || value.chars().any(|character| {
            character.is_control() && !(allow_newlines && matches!(character, '\n' | '\r' | '\t'))
        })
    {
        return Err(TravelPlanError::InvalidText);
    }
    Ok(value)
}

fn normalize_keys(keys: Vec<String>) -> Result<Vec<String>, TravelPlanError> {
    if keys.len() > MAX_PLAN_ITEMS {
        return Err(TravelPlanError::TooManyItems);
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(keys.len());
    for key in keys {
        let key = key.trim().to_string();
        if !is_valid_key(&key) {
            return Err(TravelPlanError::InvalidItemKey);
        }
        if seen.insert(key.clone()) {
            normalized.push(key);
        }
    }
    Ok(normalized)
}

fn is_valid_key(key: &str) -> bool {
    !key.is_empty() && key.chars().count() <= MAX_KEY_CHARS && !key.chars().any(char::is_control)
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes.iter().enumerate().any(|(index, byte)| {
            if index == 4 || index == 7 {
                *byte != b'-'
            } else {
                !byte.is_ascii_digit()
            }
        })
    {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        value[0..4].parse::<u16>(),
        value[5..7].parse::<u8>(),
        value[8..10].parse::<u8>(),
    ) else {
        return false;
    };
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        2 if leap_year => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=max_day).contains(&day)
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum TravelPlanError {
    #[error("行程城市无效")]
    InvalidCity,
    #[error("行程天数必须在 1 到 30 天之间")]
    InvalidDays,
    #[error("行程版本无效，请重新载入后保存")]
    InvalidRevision,
    #[error("行程文本超出长度限制或包含非法字符")]
    InvalidText,
    #[error("地点名称无效")]
    InvalidPlaceName,
    #[error("自定义地点数量超出限制")]
    TooManyCustomPlaces,
    #[error("出行日期范围无效")]
    InvalidDateRange,
    #[error("行程景点数量超出限制")]
    TooManyItems,
    #[error("景点标识无效")]
    InvalidItemKey,
    #[error("景点的行程日安排无效")]
    InvalidAssignment,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{TravelPlan, TravelPlanContext, TravelPlanError};
    use crate::travel::TravelDateRange;

    fn plan() -> TravelPlan {
        TravelPlan {
            context: TravelPlanContext {
                city: "浙江省杭州市".to_string(),
                days: 2,
                date_range: Some(TravelDateRange {
                    start: "2026-10-01".to_string(),
                    end: "2026-10-02".to_string(),
                }),
            },
            selected_keys: vec!["poi-1".to_string(), "poi-1".to_string()],
            assigned_days: BTreeMap::from([("poi-1".to_string(), 2)]),
            must_visit_keys: vec!["poi-1".to_string()],
            updated_at: 123,
            ..TravelPlan::default()
        }
    }

    #[test]
    fn validates_and_normalizes_a_plan_context_and_items() {
        let validated = plan().validate().expect("valid plan");
        assert_eq!(validated.context.city, "杭州");
        assert_eq!(validated.selected_keys, ["poi-1"]);
        assert_eq!(validated.updated_at, 0);
        assert_eq!(validated.revision, 0);
    }

    #[test]
    fn rejects_invalid_date_and_day_assignments() {
        let mut invalid_date = plan();
        invalid_date
            .context
            .date_range
            .as_mut()
            .expect("range")
            .start = "2026-02-29".to_string();
        assert_eq!(
            invalid_date.validate().unwrap_err(),
            TravelPlanError::InvalidDateRange
        );

        let mut invalid_day = plan();
        invalid_day.assigned_days.insert("poi-1".to_string(), 3);
        assert_eq!(
            invalid_day.validate().unwrap_err(),
            TravelPlanError::InvalidAssignment
        );
    }

    #[test]
    fn validates_semantic_trip_details_and_custom_stops() {
        let mut value = plan();
        value.title = "  西湖周末行  ".to_string();
        value.departure = "杭州东站".to_string();
        value.notes = "带上雨伞\n预留午休".to_string();
        value.custom_places.insert(
            "custom:walk".to_string(),
            super::TravelPlanPlace {
                name: " 河坊街 ".to_string(),
                note: "晚餐后步行".to_string(),
            },
        );
        let validated = value.validate().expect("valid semantic plan");
        assert_eq!(validated.title, "西湖周末行");
        assert_eq!(validated.custom_places["custom:walk"].name, "河坊街");
        assert!(validated.notes.contains('\n'));
    }

    #[test]
    fn rejects_invalid_revision_and_custom_place_name() {
        let mut invalid_revision = plan();
        invalid_revision.revision = -1;
        assert_eq!(
            invalid_revision.validate(),
            Err(TravelPlanError::InvalidRevision)
        );

        let mut invalid_place = plan();
        invalid_place.custom_places.insert(
            "custom:blank".to_string(),
            super::TravelPlanPlace::default(),
        );
        assert_eq!(
            invalid_place.validate(),
            Err(TravelPlanError::InvalidPlaceName)
        );
    }

    #[test]
    fn must_visit_wishes_may_remain_outside_the_current_itinerary() {
        let mut wish_list = plan();
        wish_list.must_visit_keys = vec!["unselected-wish".to_string()];
        assert_eq!(
            wish_list
                .validate()
                .expect("valid wish list")
                .must_visit_keys,
            ["unselected-wish"]
        );
    }
}
