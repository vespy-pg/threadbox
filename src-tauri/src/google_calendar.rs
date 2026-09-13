//! Google Calendar adapter behind Threadbox's local capability checks.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Days, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    database::Database,
    error::{AppError, AppResult},
    google,
    integrations::{
        CAPABILITY_CALENDAR_FREE_BUSY, CAPABILITY_CALENDAR_READ, CAPABILITY_CALENDAR_WRITE,
    },
};

const CALENDAR_API: &str = "https://www.googleapis.com/calendar/v3";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalCalendar {
    pub id: String,
    pub summary: String,
    pub primary: bool,
    pub access_role: String,
    pub time_zone: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalCalendarEvent {
    pub id: String,
    pub calendar_id: String,
    pub summary: String,
    pub description: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub time_zone: Option<String>,
    pub html_link: Option<String>,
    pub conference_link: Option<String>,
    pub attendees: Vec<String>,
    pub etag: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEventDraft {
    pub organization_id: String,
    pub project_id: Option<String>,
    pub connection_id: String,
    pub calendar_id: String,
    pub summary: String,
    #[serde(default)]
    pub description: String,
    pub start: String,
    pub end: String,
    pub time_zone: String,
    #[serde(default)]
    pub attendees: Vec<String>,
    #[serde(default)]
    pub add_google_meet: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindTimeInput {
    pub connection_id: String,
    pub calendar_ids: Vec<String>,
    pub time_min: String,
    pub time_max: String,
    pub duration_minutes: i64,
    #[serde(default)]
    pub buffer_minutes: i64,
    pub time_zone: String,
    pub workday_start: String,
    pub workday_end: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableSlot {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Deserialize)]
struct CalendarListResponse {
    #[serde(default)]
    items: Vec<CalendarListItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarListItem {
    id: String,
    summary: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    access_role: String,
    time_zone: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EventListResponse {
    #[serde(default)]
    items: Vec<GoogleEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleEvent {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    description: String,
    start: GoogleEventTime,
    end: GoogleEventTime,
    html_link: Option<String>,
    hangout_link: Option<String>,
    #[serde(default)]
    attendees: Vec<GoogleAttendee>,
    etag: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleEventTime {
    date_time: Option<String>,
    date: Option<String>,
    time_zone: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleAttendee {
    email: String,
}

#[derive(Debug, Deserialize)]
struct FreeBusyResponse {
    #[serde(default)]
    calendars: HashMap<String, FreeBusyCalendar>,
}

#[derive(Debug, Deserialize)]
struct FreeBusyCalendar {
    #[serde(default)]
    busy: Vec<BusyPeriod>,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct BusyPeriod {
    start: String,
    end: String,
}

impl Database {
    pub fn google_calendars(
        &self,
        connection_id: &str,
        client_id: &str,
    ) -> AppResult<Vec<ExternalCalendar>> {
        self.require_integration_capability(connection_id, CAPABILITY_CALENDAR_READ)?;
        let token = google::access_token(connection_id, client_id)?;
        let response: CalendarListResponse = Client::new()
            .get(format!("{CALENDAR_API}/users/me/calendarList"))
            .bearer_auth(token)
            .query(&[("minAccessRole", "reader")])
            .send()?
            .error_for_status()?
            .json()?;
        Ok(response
            .items
            .into_iter()
            .map(|item| ExternalCalendar {
                id: item.id,
                summary: item.summary,
                primary: item.primary,
                access_role: item.access_role,
                time_zone: item.time_zone,
            })
            .collect())
    }

    pub fn google_calendar_events(
        &self,
        connection_id: &str,
        client_id: &str,
        calendar_id: &str,
        time_min: &str,
        time_max: &str,
    ) -> AppResult<Vec<ExternalCalendarEvent>> {
        self.require_integration_capability(connection_id, CAPABILITY_CALENDAR_READ)?;
        parse_rfc3339(time_min, "calendar range start")?;
        parse_rfc3339(time_max, "calendar range end")?;
        let token = google::access_token(connection_id, client_id)?;
        let encoded_calendar = urlencoding::encode(calendar_id);
        let response: EventListResponse = Client::new()
            .get(format!(
                "{CALENDAR_API}/calendars/{encoded_calendar}/events"
            ))
            .bearer_auth(token)
            .query(&[
                ("timeMin", time_min),
                ("timeMax", time_max),
                ("singleEvents", "true"),
                ("orderBy", "startTime"),
                ("maxResults", "250"),
            ])
            .send()?
            .error_for_status()?
            .json()?;
        response
            .items
            .into_iter()
            .map(|event| map_event(calendar_id, event))
            .collect()
    }

    pub fn create_google_calendar_event(
        &self,
        client_id: &str,
        draft: &CalendarEventDraft,
    ) -> AppResult<ExternalCalendarEvent> {
        self.require_integration_capability(&draft.connection_id, CAPABILITY_CALENDAR_WRITE)?;
        if draft.summary.trim().is_empty() {
            return Err(AppError::InvalidInput("An event title is required".into()));
        }
        let start = parse_rfc3339(&draft.start, "event start")?;
        let end = parse_rfc3339(&draft.end, "event end")?;
        if end <= start {
            return Err(AppError::InvalidInput(
                "Event end must be after its start".into(),
            ));
        }
        if draft
            .attendees
            .iter()
            .any(|email| !email.contains('@') || email.chars().any(char::is_whitespace))
        {
            return Err(AppError::InvalidInput(
                "Every attendee must be an email address".into(),
            ));
        }
        draft.time_zone.parse::<Tz>().map_err(|_| {
            AppError::InvalidInput("Use an IANA time zone such as Europe/Warsaw".into())
        })?;
        let token = google::access_token(&draft.connection_id, client_id)?;
        let encoded_calendar = urlencoding::encode(&draft.calendar_id);
        let mut request = Client::new()
            .post(format!(
                "{CALENDAR_API}/calendars/{encoded_calendar}/events"
            ))
            .bearer_auth(token)
            .query(&[("sendUpdates", "all")]);
        let conference = draft
            .add_google_meet
            .then(|| json!({"createRequest": {"requestId": Uuid::new_v4().to_string()}}));
        if conference.is_some() {
            request = request.query(&[("conferenceDataVersion", "1")]);
        }
        let event: GoogleEvent = request
            .json(&json!({
                "summary": draft.summary.trim(),
                "description": draft.description,
                "start": {"dateTime": draft.start, "timeZone": draft.time_zone},
                "end": {"dateTime": draft.end, "timeZone": draft.time_zone},
                "attendees": draft.attendees.iter().map(|email| json!({"email": email})).collect::<Vec<_>>(),
                "conferenceData": conference,
            }))
            .send()?
            .error_for_status()?
            .json()?;
        map_event(&draft.calendar_id, event)
    }

    pub fn find_google_calendar_time(
        &self,
        client_id: &str,
        input: &FindTimeInput,
    ) -> AppResult<Vec<AvailableSlot>> {
        self.require_integration_capability(&input.connection_id, CAPABILITY_CALENDAR_FREE_BUSY)?;
        if input.calendar_ids.is_empty() || input.calendar_ids.len() > 50 {
            return Err(AppError::InvalidInput(
                "Choose between one and fifty calendars".into(),
            ));
        }
        if !(5..=480).contains(&input.duration_minutes)
            || !(0..=240).contains(&input.buffer_minutes)
        {
            return Err(AppError::InvalidInput(
                "Duration must be 5-480 minutes and buffer 0-240 minutes".into(),
            ));
        }
        let time_min = parse_rfc3339(&input.time_min, "availability range start")?;
        let time_max = parse_rfc3339(&input.time_max, "availability range end")?;
        if time_max <= time_min {
            return Err(AppError::InvalidInput(
                "Availability range end must be after its start".into(),
            ));
        }
        let timezone = input.time_zone.parse::<Tz>().map_err(|_| {
            AppError::InvalidInput("Use an IANA time zone such as Europe/Warsaw".into())
        })?;
        let workday_start = parse_time(&input.workday_start)?;
        let workday_end = parse_time(&input.workday_end)?;
        if workday_end <= workday_start {
            return Err(AppError::InvalidInput(
                "Workday end must be after its start".into(),
            ));
        }
        let token = google::access_token(&input.connection_id, client_id)?;
        let response: FreeBusyResponse = Client::new()
            .post(format!("{CALENDAR_API}/freeBusy"))
            .bearer_auth(token)
            .json(&json!({
                "timeMin": input.time_min,
                "timeMax": input.time_max,
                "timeZone": input.time_zone,
                "items": input.calendar_ids.iter().map(|id| json!({"id": id})).collect::<Vec<_>>(),
            }))
            .send()?
            .error_for_status()?
            .json()?;
        let mut busy = Vec::new();
        for calendar_id in &input.calendar_ids {
            let calendar = response.calendars.get(calendar_id).ok_or_else(|| {
                AppError::InvalidInput(format!("Google returned no availability for {calendar_id}"))
            })?;
            if !calendar.errors.is_empty() {
                return Err(AppError::InvalidInput(format!(
                    "Google did not allow availability access for {calendar_id}"
                )));
            }
            for period in &calendar.busy {
                busy.push((
                    parse_rfc3339(&period.start, "busy period start")?,
                    parse_rfc3339(&period.end, "busy period end")?,
                ));
            }
        }
        available_slots(
            time_min,
            time_max,
            timezone,
            workday_start,
            workday_end,
            input.duration_minutes,
            input.buffer_minutes,
            &busy,
        )
    }
}

fn map_event(calendar_id: &str, event: GoogleEvent) -> AppResult<ExternalCalendarEvent> {
    let start = event
        .start
        .date_time
        .clone()
        .or_else(|| event.start.date.clone())
        .ok_or_else(|| AppError::InvalidInput("Google event has no start".into()))?;
    let end = event
        .end
        .date_time
        .clone()
        .or_else(|| event.end.date.clone())
        .ok_or_else(|| AppError::InvalidInput("Google event has no end".into()))?;
    Ok(ExternalCalendarEvent {
        id: event.id,
        calendar_id: calendar_id.into(),
        summary: if event.summary.trim().is_empty() {
            "Untitled calendar event".into()
        } else {
            event.summary
        },
        description: event.description,
        start,
        end,
        all_day: event.start.date_time.is_none(),
        time_zone: event.start.time_zone,
        html_link: event.html_link,
        conference_link: event.hangout_link,
        attendees: event.attendees.into_iter().map(|item| item.email).collect(),
        etag: event.etag,
    })
}

fn parse_rfc3339(value: &str, label: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| AppError::InvalidInput(format!("Invalid {label}: {error}")))
}

fn parse_time(value: &str) -> AppResult<NaiveTime> {
    NaiveTime::parse_from_str(value, "%H:%M")
        .map_err(|_| AppError::InvalidInput("Working hours must use HH:MM".into()))
}

#[allow(clippy::too_many_arguments)]
fn available_slots(
    range_start: DateTime<Utc>,
    range_end: DateTime<Utc>,
    timezone: Tz,
    workday_start: NaiveTime,
    workday_end: NaiveTime,
    duration_minutes: i64,
    buffer_minutes: i64,
    busy: &[(DateTime<Utc>, DateTime<Utc>)],
) -> AppResult<Vec<AvailableSlot>> {
    let duration = chrono::Duration::minutes(duration_minutes);
    let buffer = chrono::Duration::minutes(buffer_minutes);
    let mut date: NaiveDate = range_start.with_timezone(&timezone).date_naive();
    let last_date = range_end.with_timezone(&timezone).date_naive();
    let mut result = Vec::new();
    while date <= last_date && result.len() < 100 {
        if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
            date = date
                .checked_add_days(Days::new(1))
                .ok_or_else(|| AppError::InvalidInput("Availability range is too large".into()))?;
            continue;
        }
        let local_start = timezone
            .from_local_datetime(&date.and_time(workday_start))
            .earliest()
            .ok_or_else(|| {
                AppError::InvalidInput("Working hours cross an invalid local time".into())
            })?
            .with_timezone(&Utc)
            .max(range_start);
        let local_end = timezone
            .from_local_datetime(&date.and_time(workday_end))
            .latest()
            .ok_or_else(|| {
                AppError::InvalidInput("Working hours cross an invalid local time".into())
            })?
            .with_timezone(&Utc)
            .min(range_end);
        let mut candidate = local_start;
        while candidate + duration <= local_end && result.len() < 100 {
            let candidate_end = candidate + duration;
            let collides = busy
                .iter()
                .any(|(start, end)| candidate < *end + buffer && candidate_end > *start - buffer);
            if !collides {
                result.push(AvailableSlot {
                    start: candidate.to_rfc3339(),
                    end: candidate_end.to_rfc3339(),
                });
            }
            candidate += chrono::Duration::minutes(15);
        }
        date = date
            .checked_add_days(Days::new(1))
            .ok_or_else(|| AppError::InvalidInput("Availability range is too large".into()))?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_slots_respect_busy_time_buffers_and_daylight_saving() {
        let range_start = parse_rfc3339("2026-10-23T00:00:00Z", "start").unwrap();
        let range_end = parse_rfc3339("2026-10-27T23:00:00Z", "end").unwrap();
        let busy = vec![(
            parse_rfc3339("2026-10-23T08:00:00Z", "busy start").unwrap(),
            parse_rfc3339("2026-10-23T09:00:00Z", "busy end").unwrap(),
        )];
        let slots = available_slots(
            range_start,
            range_end,
            chrono_tz::Europe::Warsaw,
            parse_time("09:00").unwrap(),
            parse_time("11:00").unwrap(),
            30,
            15,
            &busy,
        )
        .unwrap();
        assert!(!slots.is_empty());
        assert!(slots.iter().all(|slot| slot.start < slot.end));
        assert!(slots.iter().any(|slot| slot.start.contains("T08:00:00")));
        assert!(slots.iter().any(|slot| slot.start.contains("T09:00:00")));
    }
}
