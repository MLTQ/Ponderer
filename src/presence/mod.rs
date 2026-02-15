use chrono::{Datelike, Local, Timelike, Weekday};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Foundation-only presence monitor.
///
/// ll.1 intentionally keeps this as a lightweight stub so schema/types can land
/// without changing agent behavior. Real platform sampling is introduced later.
pub struct PresenceMonitor {
    session_start: Instant,
    last_interaction: Option<Instant>,
}

impl PresenceMonitor {
    pub fn new() -> Self {
        Self {
            session_start: Instant::now(),
            last_interaction: None,
        }
    }

    pub fn record_interaction(&mut self) {
        self.last_interaction = Some(Instant::now());
    }

    pub fn sample(&self) -> PresenceState {
        let now = Instant::now();
        let time_since_interaction = self
            .last_interaction
            .map(|instant| now.saturating_duration_since(instant))
            .unwrap_or_else(|| now.saturating_duration_since(self.session_start));

        PresenceState {
            user_idle_seconds: time_since_interaction.as_secs(),
            time_since_interaction,
            session_duration: now.saturating_duration_since(self.session_start),
            time_context: TimeContext::now(),
            system_load: SystemLoad {
                cpu_percent: 0.0,
                memory_percent: 0.0,
                gpu_temp_celsius: None,
                gpu_util_percent: None,
            },
            active_processes: Vec::new(),
        }
    }
}

impl Default for PresenceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceState {
    pub user_idle_seconds: u64,
    #[serde(with = "duration_seconds")]
    pub time_since_interaction: Duration,
    #[serde(with = "duration_seconds")]
    pub session_duration: Duration,
    pub time_context: TimeContext,
    pub system_load: SystemLoad,
    pub active_processes: Vec<InterestingProcess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeContext {
    pub local_hour: u8,
    pub local_minute: u8,
    pub day_of_week: Weekday,
    pub is_weekend: bool,
    pub is_late_night: bool,
    pub is_deep_night: bool,
    pub approx_work_hours: bool,
}

impl TimeContext {
    pub fn now() -> Self {
        let now = Local::now();
        let hour = now.hour() as u8;
        let minute = now.minute() as u8;
        let weekday = now.weekday();
        let is_weekend = matches!(weekday, Weekday::Sat | Weekday::Sun);

        Self {
            local_hour: hour,
            local_minute: minute,
            day_of_week: weekday,
            is_weekend,
            is_late_night: hour >= 23 || hour < 6,
            is_deep_night: (2..5).contains(&hour),
            approx_work_hours: !is_weekend && (8..=18).contains(&hour),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemLoad {
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub gpu_temp_celsius: Option<f32>,
    pub gpu_util_percent: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterestingProcess {
    pub name: String,
    pub category: ProcessCategory,
    pub cpu_percent: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessCategory {
    Development,
    Creative,
    Research,
    Communication,
    Media,
    System,
}

mod duration_seconds {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_returns_nonzero_session_duration() {
        let monitor = PresenceMonitor::new();
        let state = monitor.sample();
        assert!(state.session_duration.as_secs() < 2);
        assert!(state.time_context.local_hour <= 23);
    }
}
