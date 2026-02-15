use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A private inner-life note captured by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub entry_type: JournalEntryType,
    pub content: String,
    pub context: JournalContext,
    pub related_concerns: Vec<String>,
    pub mood_at_time: Option<JournalMood>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalEntryType {
    Observation,
    Reflection,
    Realization,
    Intention,
    Question,
    Memory,
    Gratitude,
    Frustration,
}

impl JournalEntryType {
    pub fn as_db_str(self) -> &'static str {
        match self {
            JournalEntryType::Observation => "observation",
            JournalEntryType::Reflection => "reflection",
            JournalEntryType::Realization => "realization",
            JournalEntryType::Intention => "intention",
            JournalEntryType::Question => "question",
            JournalEntryType::Memory => "memory",
            JournalEntryType::Gratitude => "gratitude",
            JournalEntryType::Frustration => "frustration",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "reflection" => JournalEntryType::Reflection,
            "realization" => JournalEntryType::Realization,
            "intention" => JournalEntryType::Intention,
            "question" => JournalEntryType::Question,
            "memory" => JournalEntryType::Memory,
            "gratitude" => JournalEntryType::Gratitude,
            "frustration" => JournalEntryType::Frustration,
            _ => JournalEntryType::Observation,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JournalContext {
    pub trigger: String,
    pub user_state_at_time: String,
    pub time_of_day: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalMood {
    pub valence: f32,
    pub arousal: f32,
}
