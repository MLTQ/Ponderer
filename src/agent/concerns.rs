use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concern {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub last_touched: DateTime<Utc>,
    pub summary: String,
    pub concern_type: ConcernType,
    pub salience: Salience,
    pub my_thoughts: String,
    pub related_memory_keys: Vec<String>,
    pub context: ConcernContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ConcernType {
    CollaborativeProject {
        project_name: String,
        my_role: String,
    },
    HouseholdAwareness {
        category: String,
    },
    SystemHealth {
        component: String,
        monitoring_since: DateTime<Utc>,
    },
    PersonalInterest {
        topic: String,
        curiosity_level: f32,
    },
    Reminder {
        trigger_time: Option<DateTime<Utc>>,
        trigger_condition: Option<String>,
    },
    OngoingConversation {
        thread_id: String,
        with_whom: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Salience {
    Active,
    Monitoring,
    Background,
    Dormant,
}

impl Salience {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Salience::Active => "active",
            Salience::Monitoring => "monitoring",
            Salience::Background => "background",
            Salience::Dormant => "dormant",
        }
    }

    pub fn from_db(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "monitoring" => Salience::Monitoring,
            "background" => Salience::Background,
            "dormant" => Salience::Dormant,
            _ => Salience::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConcernContext {
    pub how_it_started: String,
    pub key_events: Vec<String>,
    pub last_update_reason: String,
}
