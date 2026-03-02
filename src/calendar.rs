use std::time::Duration;

use chrono::{Days, Utc};
use moka::future::Cache;
use serde_json::{json, Value};

use crate::auth::CalendarHubType;
use crate::error::AppError;

const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";

pub struct CalendarClient {
    hub: CalendarHubType,
    memory_cache: Cache<String, Value>,
}

impl std::fmt::Debug for CalendarClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalendarClient").finish_non_exhaustive()
    }
}

impl CalendarClient {
    pub fn new(hub: CalendarHubType) -> Self {
        let memory_cache = Cache::builder()
            .max_capacity(200)
            .time_to_live(Duration::from_secs(300))
            .build();

        Self { hub, memory_cache }
    }

    /// List all calendars the authenticated user has access to.
    pub async fn list_calendars(&self) -> Result<Value, AppError> {
        let key = "calendar_list".to_string();
        if let Some(cached) = self.memory_cache.get(&key).await {
            tracing::debug!("memory cache hit: {key}");
            return Ok(cached);
        }
        tracing::debug!("memory cache miss: {key}");

        let (_resp, list) = self
            .hub
            .calendar_list()
            .list()
            .clear_scopes()
            .add_scope(CALENDAR_SCOPE)
            .doit()
            .await
            .map_err(|e| AppError::GoogleApi(e.to_string()))?;

        let calendars: Vec<Value> = list
            .items
            .unwrap_or_default()
            .iter()
            .map(|cal| {
                json!({
                    "id": cal.id,
                    "summary": cal.summary,
                    "description": cal.description,
                    "primary": cal.primary,
                    "accessRole": cal.access_role,
                    "timeZone": cal.time_zone,
                    "backgroundColor": cal.background_color,
                })
            })
            .collect();

        let value = serde_json::to_value(&calendars).map_err(AppError::Json)?;
        self.memory_cache.insert(key, value.clone()).await;
        Ok(value)
    }

    /// List upcoming events for a calendar.
    pub async fn list_events(
        &self,
        calendar_id: &str,
        days_ahead: u32,
    ) -> Result<Value, AppError> {
        let key = format!("events:{calendar_id}:{days_ahead}");
        if let Some(cached) = self.memory_cache.get(&key).await {
            tracing::debug!("memory cache hit: {key}");
            return Ok(cached);
        }
        tracing::debug!("memory cache miss: {key}");

        let now = Utc::now();
        let until = now
            .checked_add_days(Days::new(days_ahead as u64))
            .unwrap_or(now);

        let (_resp, list) = self
            .hub
            .events()
            .list(calendar_id)
            .time_min(now)
            .time_max(until)
            .single_events(true)
            .order_by("startTime")
            .max_results(100)
            .clear_scopes()
            .add_scope(CALENDAR_SCOPE)
            .doit()
            .await
            .map_err(|e| AppError::GoogleApi(e.to_string()))?;

        let events: Vec<Value> = list
            .items
            .unwrap_or_default()
            .iter()
            .map(|evt| {
                json!({
                    "id": evt.id,
                    "summary": evt.summary,
                    "description": evt.description,
                    "location": evt.location,
                    "start": evt.start,
                    "end": evt.end,
                    "status": evt.status,
                    "htmlLink": evt.html_link,
                    "hangoutLink": evt.hangout_link,
                    "attendees": evt.attendees.as_ref().map(|a| a.iter().map(|att| json!({
                        "email": att.email,
                        "displayName": att.display_name,
                        "responseStatus": att.response_status,
                        "organizer": att.organizer,
                        "self": att.self_,
                    })).collect::<Vec<_>>()),
                    "organizer": evt.organizer.as_ref().map(|o| json!({
                        "email": o.email,
                        "displayName": o.display_name,
                    })),
                    "created": evt.created,
                    "updated": evt.updated,
                    "recurrence": evt.recurrence,
                    "recurringEventId": evt.recurring_event_id,
                })
            })
            .collect();

        let value = serde_json::to_value(&events).map_err(AppError::Json)?;
        self.memory_cache.insert(key, value.clone()).await;
        Ok(value)
    }

    /// Get full details for a single event.
    pub async fn get_event(
        &self,
        calendar_id: &str,
        event_id: &str,
    ) -> Result<Value, AppError> {
        let key = format!("event:{calendar_id}:{event_id}");
        if let Some(cached) = self.memory_cache.get(&key).await {
            tracing::debug!("memory cache hit: {key}");
            return Ok(cached);
        }
        tracing::debug!("memory cache miss: {key}");

        let (_resp, event) = self
            .hub
            .events()
            .get(calendar_id, event_id)
            .clear_scopes()
            .add_scope(CALENDAR_SCOPE)
            .doit()
            .await
            .map_err(|e| AppError::GoogleApi(e.to_string()))?;

        let value = serde_json::to_value(&event).map_err(AppError::Json)?;
        self.memory_cache.insert(key, value.clone()).await;
        Ok(value)
    }
}
