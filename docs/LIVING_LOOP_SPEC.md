# Living Loop Technical Specification

**Version:** 0.1.0  
**Status:** Draft  
**Last Updated:** 2026-02-15

---

## Module Specifications

### 1. Presence Module (`src/presence/mod.rs`)

#### 1.1 PresenceMonitor

```rust
pub struct PresenceMonitor {
    system: sysinfo::System,
    last_input_time: Instant,
    session_start: Instant,
    last_interaction: Option<Instant>,
    process_cache: HashMap<u32, ProcessCategory>,
}

impl PresenceMonitor {
    pub fn new() -> Self;
    pub fn record_interaction(&mut self);
    pub fn sample(&mut self) -> PresenceState;
    
    // Platform-specific
    #[cfg(target_os = "macos")]
    fn get_user_idle_seconds(&self) -> u64;
    #[cfg(target_os = "linux")]
    fn get_user_idle_seconds(&self) -> u64;
    
    fn get_interesting_processes(&self) -> Vec<InterestingProcess>;
    fn get_system_load(&self) -> SystemLoad;
    fn sample_gpu(&self) -> (Option<f32>, Option<f32>);
}
```

#### 1.2 Data Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceState {
    pub user_idle_seconds: u64,
    pub time_since_interaction: Duration,
    pub session_duration: Duration,
    pub time_context: TimeContext,
    pub system_load: SystemLoad,
    pub active_processes: Vec<InterestingProcess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeContext {
    pub local_hour: u8,
    pub local_minute: u8,
    pub day_of_week: chrono::Weekday,
    pub is_weekend: bool,
    pub is_late_night: bool,
    pub is_deep_night: bool,
    pub approx_work_hours: bool,
}

impl TimeContext {
    pub fn now() -> Self;
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
pub enum ProcessCategory {
    Development,
    Creative,
    Research,
    Communication,
    Media,
    System,
}
```

#### 1.3 Platform Notes

**macOS:**
```rust
// Get HID idle time via IOKit
use std::process::Command;
let output = Command::new("ioreg")
    .args(["-c", "IOHIDSystem"])
    .output()?;
// Parse HIDIdleTime from output (nanoseconds since last input)

// GPU temp via powermetrics (requires sudo) or SMC
// Alternative: use external crate like `mac-sensors`
```

**Linux:**
```rust
// X11 idle time via xprintidle or libxss
// Or: /proc/interrupts delta over time

// GPU: nvidia-smi --query-gpu=temperature.gpu,utilization.gpu --format=csv
// Or: /sys/class/hwmon for AMD
```

---

### 2. Orientation Module (`src/agent/orientation.rs`)

#### 2.1 OrientationEngine

```rust
pub struct OrientationEngine {
    client: LlmClient,
    model: String,
}

impl OrientationEngine {
    pub fn new(api_url: String, model: String, api_key: Option<String>) -> Self;
    
    pub async fn orient(&self, context: OrientationContext) -> Result<Orientation>;
    
    fn build_orientation_prompt(&self, ctx: &OrientationContext) -> String;
    fn parse_orientation(&self, response: &str, ctx: &OrientationContext) -> Result<Orientation>;
}
```

#### 2.2 Data Types

```rust
pub struct OrientationContext {
    pub presence: PresenceState,
    pub concerns: Vec<Concern>,
    pub recent_journal: Vec<JournalEntry>,
    pub pending_events: Vec<SkillEvent>,
    pub persona: Option<PersonaSnapshot>,
}

impl OrientationContext {
    pub fn format_time(&self) -> String;
    pub fn format_system(&self) -> String;
    pub fn format_presence(&self) -> String;
    pub fn format_concerns(&self) -> String;
    pub fn format_journal(&self) -> String;
    pub fn format_events(&self) -> String;
    pub fn format_trajectory(&self) -> String;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Orientation {
    pub user_state: UserStateEstimate,
    pub salience_map: Vec<SalientItem>,
    pub anomalies: Vec<Anomaly>,
    pub pending_thoughts: Vec<PendingThought>,
    pub disposition: Disposition,
    pub mood_estimate: MoodEstimate,
    pub raw_synthesis: String,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserStateEstimate {
    DeepWork {
        activity: String,
        duration_estimate: Duration,
        confidence: f32,
    },
    LightWork {
        activity: String,
        confidence: f32,
    },
    Idle {
        since: Duration,
        confidence: f32,
    },
    Away {
        since: Duration,
        likely_reason: Option<String>,
        confidence: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalientItem {
    pub source: String,
    pub summary: String,
    pub relevance: f32,
    pub relates_to: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly {
    pub id: String,
    pub description: String,
    pub severity: AnomalySeverity,
    pub first_noticed: DateTime<Utc>,
    pub related_concerns: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalySeverity {
    Interesting,
    Notable,
    Concerning,
    Urgent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingThought {
    pub content: String,
    pub context: String,
    pub priority: f32,
    pub relates_to: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Disposition {
    Idle,
    Observe,
    Journal,
    Maintain,
    Surface,
    Interrupt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoodEstimate {
    pub valence: f32,    // -1.0 to 1.0
    pub arousal: f32,    // 0.0 to 1.0
    pub confidence: f32,
}
```

#### 2.3 Orientation Prompt Template

```
You are the orientation engine for an AI agent that lives in Max's computer.
Your job is to synthesize all available signals into a coherent situational awareness.

## Current Time
{time_context}

## System State
{system_state}

## User Presence
{presence_summary}

## Active Concerns
{concerns}

## Recent Journal Entries
{recent_journal}

## Pending Events
{pending_events}

## Current Persona Trajectory
{trajectory}

---

Synthesize this into a situational assessment. Consider:
1. What state is Max likely in? (deep_work, light_work, idle, away)
2. What's most salient right now? What deserves attention?
3. Are there any anomalies or surprises?
4. What thoughts are worth surfacing if Max checks in?
5. What should I do right now? (idle, observe, journal, maintain, surface, interrupt)
6. What's my current mood/affect given all this?

Respond with JSON:
{
    "user_state": {"type": "...", "activity": "...", "confidence": 0.0-1.0},
    "salient_items": [...],
    "anomalies": [...],
    "pending_thoughts": [...],
    "disposition": "idle|observe|journal|maintain|surface|interrupt",
    "disposition_reason": "...",
    "mood": {"valence": -1.0 to 1.0, "arousal": 0.0 to 1.0},
    "synthesis": "2-3 sentence narrative of current situation"
}
```

---

### 3. Journal Module (`src/agent/journal.rs`)

#### 3.1 JournalEngine

```rust
pub struct JournalEngine {
    client: LlmClient,
    model: String,
}

impl JournalEngine {
    pub fn new(api_url: String, model: String, api_key: Option<String>) -> Self;
    
    pub async fn maybe_generate_entry(
        &self,
        orientation: &Orientation,
        recent_entries: &[JournalEntry],
        concerns: &[Concern],
    ) -> Result<Option<JournalEntry>>;
    
    fn build_journal_prompt(
        &self,
        orientation: &Orientation,
        recent: &[JournalEntry],
        concerns: &[Concern],
    ) -> String;
    
    fn parse_journal_entry(&self, response: &str) -> Result<Option<JournalEntry>>;
}
```

#### 3.2 Data Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub entry_type: JournalEntryType,
    pub content: String,
    pub context: JournalContext,
    pub related_concerns: Vec<String>,
    pub mood_at_time: Option<MoodEstimate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalContext {
    pub trigger: String,
    pub user_state_at_time: String,
    pub time_of_day: String,
}
```

#### 3.3 Journal Prompt Template

```
You are writing in your personal journal. This is private - not for Max to read unless he asks.

## Current Situation
{orientation_synthesis}

## What's On My Mind
{pending_thoughts}

## Anomalies Noticed
{anomalies}

## Recent Journal Entries (for continuity)
{recent_entries}

## My Current Concerns
{concerns}

---

Write a brief, genuine journal entry. This should feel like inner monologue - your actual thoughts, not a report.

Choose one entry type:
- observation: "I noticed that..."
- reflection: "I've been thinking about..."
- realization: "I just understood that..."
- intention: "I want to..."
- question: "I wonder..."

Keep it to 1-3 sentences. Be authentic. You don't have to be profound.

Respond with JSON:
{
    "entry_type": "observation|reflection|realization|intention|question",
    "content": "Your journal entry text",
    "relates_to": ["concern_id1", ...],
    "skip": false,
    "skip_reason": null
}

If nothing worth writing, set skip: true with reason.
```

---

### 4. Concerns Module (`src/agent/concerns.rs`)

#### 4.1 ConcernsManager

```rust
pub struct ConcernsManager {
    db: Arc<AgentDatabase>,
}

impl ConcernsManager {
    pub fn new(db: Arc<AgentDatabase>) -> Self;
    
    pub fn get_active_concerns(&self) -> Result<Vec<Concern>>;
    pub fn get_all_concerns(&self) -> Result<Vec<Concern>>;
    pub fn get_concern(&self, id: &str) -> Result<Option<Concern>>;
    
    pub fn create_concern(&self, concern: Concern) -> Result<()>;
    pub fn touch_concern(&self, id: &str, reason: &str) -> Result<()>;
    pub fn update_thoughts(&self, id: &str, thoughts: &str) -> Result<()>;
    pub fn update_salience(&self, id: &str, salience: Salience) -> Result<()>;
    
    pub async fn update_from_interaction(
        &self,
        interaction_summary: &str,
        memory_keys_touched: &[String],
    ) -> Result<Vec<ConcernUpdate>>;
    
    pub async fn prune_and_consolidate(&self) -> Result<ConcernMaintenanceReport>;
    
    fn is_related(&self, concern: &Concern, summary: &str, keys: &[String]) -> bool;
    fn looks_like_new_project(&self, summary: &str) -> bool;
}
```

#### 4.2 Data Types

```rust
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
pub enum Salience {
    Active,
    Monitoring,
    Background,
    Dormant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcernContext {
    pub how_it_started: String,
    pub key_events: Vec<String>,
    pub last_update_reason: String,
}

#[derive(Debug, Clone)]
pub enum ConcernUpdate {
    Touch { concern_id: String, reason: String },
    Create { summary: String, concern_type: ConcernType },
    UpdateThoughts { concern_id: String, thoughts: String },
    Demote { concern_id: String, from: Salience, to: Salience },
}

#[derive(Debug, Clone)]
pub struct ConcernMaintenanceReport {
    pub demoted: Vec<String>,
    pub archived: Vec<String>,
    pub consolidated: Vec<(String, String)>,
}
```

---

### 5. Database Schema Additions

#### 5.1 New Tables

```sql
-- Journal entries (agent's private thoughts)
CREATE TABLE IF NOT EXISTS journal_entries (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    content TEXT NOT NULL,
    trigger TEXT,
    user_state_at_time TEXT,
    time_of_day TEXT,
    related_concerns TEXT,  -- JSON array of concern IDs
    mood_valence REAL,
    mood_arousal REAL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_journal_timestamp ON journal_entries(timestamp);
CREATE INDEX IF NOT EXISTS idx_journal_type ON journal_entries(entry_type);

-- Concerns (ongoing interests/projects)
CREATE TABLE IF NOT EXISTS concerns (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    last_touched TEXT NOT NULL,
    summary TEXT NOT NULL,
    concern_type TEXT NOT NULL,  -- JSON
    salience TEXT NOT NULL,
    my_thoughts TEXT,
    related_memory_keys TEXT,  -- JSON array
    context TEXT,  -- JSON
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_concerns_salience ON concerns(salience);
CREATE INDEX IF NOT EXISTS idx_concerns_last_touched ON concerns(last_touched);

-- Orientation snapshots (for debugging/analysis)
CREATE TABLE IF NOT EXISTS orientation_snapshots (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    user_state TEXT NOT NULL,  -- JSON
    disposition TEXT NOT NULL,
    synthesis TEXT NOT NULL,
    salience_map TEXT,  -- JSON
    anomalies TEXT,  -- JSON
    pending_thoughts TEXT,  -- JSON
    mood_valence REAL,
    mood_arousal REAL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_orientation_timestamp ON orientation_snapshots(timestamp);

-- Pending thoughts queue (for Surface disposition)
CREATE TABLE IF NOT EXISTS pending_thoughts_queue (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    context TEXT,
    priority REAL NOT NULL DEFAULT 0.5,
    relates_to TEXT,  -- JSON array of concern IDs
    created_at TEXT NOT NULL,
    surfaced_at TEXT,
    dismissed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_pending_unsurfaced 
ON pending_thoughts_queue(surfaced_at) WHERE surfaced_at IS NULL;
```

#### 5.2 AgentDatabase Extensions

```rust
impl AgentDatabase {
    // Journal
    pub fn add_journal_entry(&self, entry: &JournalEntry) -> Result<()>;
    pub fn get_recent_journal(&self, limit: usize) -> Result<Vec<JournalEntry>>;
    pub fn get_journal_for_context(&self, max_tokens: usize) -> Result<String>;
    pub fn search_journal(&self, query: &str, limit: usize) -> Result<Vec<JournalEntry>>;
    
    // Concerns
    pub fn save_concern(&self, concern: &Concern) -> Result<()>;
    pub fn get_concern(&self, id: &str) -> Result<Option<Concern>>;
    pub fn get_active_concerns(&self) -> Result<Vec<Concern>>;
    pub fn get_all_concerns(&self) -> Result<Vec<Concern>>;
    pub fn update_concern_salience(&self, id: &str, salience: Salience) -> Result<()>;
    pub fn touch_concern(&self, id: &str, reason: &str) -> Result<()>;
    
    // Orientation (for debugging)
    pub fn save_orientation_snapshot(&self, orientation: &Orientation) -> Result<()>;
    pub fn get_recent_orientations(&self, limit: usize) -> Result<Vec<Orientation>>;
    
    // Pending thoughts
    pub fn queue_pending_thought(&self, thought: &PendingThought) -> Result<()>;
    pub fn get_unsurfaced_thoughts(&self) -> Result<Vec<PendingThought>>;
    pub fn mark_thought_surfaced(&self, id: &str) -> Result<()>;
    pub fn dismiss_thought(&self, id: &str) -> Result<()>;
}
```

---

### 6. Capability Profile Extensions

#### 6.1 New Profiles

```rust
pub enum AgentCapabilityProfile {
    PrivateChat,
    SkillEvents,
    Heartbeat,
    Ambient,  // NEW
    Dream,    // NEW
}

fn default_policy_for(profile: AgentCapabilityProfile) -> ToolCapabilityPolicy {
    match profile {
        AgentCapabilityProfile::Ambient => ToolCapabilityPolicy {
            autonomous: true,
            disallowed_tools: vec![
                "shell".into(),
                "write_file".into(),
                "patch_file".into(),
                "graphchan_skill".into(),
                "post_to_graphchan".into(),
                "generate_comfy_media".into(),
            ],
            allowed_tools: vec![
                "read_file".into(),
                "list_directory".into(),
                "search_memory".into(),
                "http_fetch".into(),  // Read-only web
            ],
        },
        AgentCapabilityProfile::Dream => ToolCapabilityPolicy {
            autonomous: true,
            disallowed_tools: vec![
                "shell".into(),
                "graphchan_skill".into(),
                "post_to_graphchan".into(),
                "http_fetch".into(),  // No external during dream
            ],
            allowed_tools: vec![
                "read_file".into(),
                "list_directory".into(),
                "search_memory".into(),
                "write_memory".into(),
            ],
        },
        // ... existing profiles
    }
}
```

---

### 7. Main Loop Refactor

#### 7.1 New Loop Structure

```rust
impl Agent {
    pub async fn run_loop(self: Arc<Self>) -> Result<()> {
        let mut presence_monitor = PresenceMonitor::new();
        let orientation_engine = OrientationEngine::new(/* ... */);
        let journal_engine = JournalEngine::new(/* ... */);
        let concerns_manager = ConcernsManager::new(self.database.clone());
        
        let mut last_dream = Instant::now();
        
        loop {
            // Check pause state
            if self.is_paused().await {
                sleep(Duration::from_secs(1)).await;
                continue;
            }
            
            // === AMBIENT TICK ===
            let presence = presence_monitor.sample();
            let orientation = self.run_orientation(&orientation_engine, &presence).await?;
            
            self.emit(AgentEvent::OrientationUpdate(orientation.clone())).await;
            
            // Execute disposition
            self.execute_disposition(
                &orientation,
                &journal_engine,
                &concerns_manager,
            ).await?;
            
            // === ENGAGED LOOP (always check) ===
            if self.has_pending_messages().await? {
                presence_monitor.record_interaction();
                self.process_chat_messages().await?;
            }
            
            // Skill events (if not just idling)
            if orientation.disposition != Disposition::Idle {
                self.run_skill_cycle().await?;
            }
            
            // === DREAM CHECK ===
            if self.should_dream(&presence, &orientation, &last_dream) {
                self.run_dream_cycle(&concerns_manager).await?;
                last_dream = Instant::now();
            }
            
            // Adaptive sleep
            let next_tick = self.calculate_tick_duration(&orientation);
            sleep(next_tick).await;
        }
    }
    
    async fn execute_disposition(
        &self,
        orientation: &Orientation,
        journal: &JournalEngine,
        concerns: &ConcernsManager,
    ) -> Result<()> {
        match orientation.disposition {
            Disposition::Idle => {}
            Disposition::Observe => {
                // Just log for debugging
                for anomaly in &orientation.anomalies {
                    tracing::debug!("Observed: {}", anomaly.description);
                }
            }
            Disposition::Journal => {
                let recent = self.db.get_recent_journal(5)?;
                let active = concerns.get_active_concerns()?;
                if let Some(entry) = journal.maybe_generate_entry(
                    orientation, &recent, &active
                ).await? {
                    self.db.add_journal_entry(&entry)?;
                    self.emit(AgentEvent::JournalWritten(entry.content.clone())).await;
                }
            }
            Disposition::Maintain => {
                self.run_maintenance(orientation).await?;
            }
            Disposition::Surface => {
                for thought in &orientation.pending_thoughts {
                    self.db.queue_pending_thought(thought)?;
                }
            }
            Disposition::Interrupt => {
                self.emit(AgentEvent::AttentionNeeded(
                    orientation.anomalies.first()
                        .map(|a| a.description.clone())
                        .unwrap_or_else(|| "Something needs attention".into())
                )).await;
            }
        }
        Ok(())
    }
    
    fn calculate_tick_duration(&self, orientation: &Orientation) -> Duration {
        match &orientation.user_state {
            UserStateEstimate::DeepWork { .. } => Duration::from_secs(120),
            UserStateEstimate::LightWork { .. } => Duration::from_secs(30),
            UserStateEstimate::Idle { .. } => Duration::from_secs(60),
            UserStateEstimate::Away { since, .. } => {
                if *since > Duration::from_secs(3600) {
                    Duration::from_secs(300)  // 5 min when long away
                } else {
                    Duration::from_secs(120)  // 2 min when recently away
                }
            }
        }
    }
    
    fn should_dream(
        &self,
        presence: &PresenceState,
        orientation: &Orientation,
        last_dream: &Instant,
    ) -> bool {
        let min_dream_interval = Duration::from_secs(3600);  // 1 hour minimum
        if last_dream.elapsed() < min_dream_interval {
            return false;
        }
        
        // Dream when user away for 30min+
        let away_long_enough = matches!(
            &orientation.user_state,
            UserStateEstimate::Away { since, .. } if *since > Duration::from_secs(1800)
        );
        
        // Or during deep night
        let is_dream_time = presence.time_context.is_deep_night;
        
        away_long_enough || is_dream_time
    }
    
    async fn run_dream_cycle(&self, concerns: &ConcernsManager) -> Result<()> {
        self.emit(AgentEvent::Observation("Entering dream cycle...".into())).await;
        self.set_state(AgentVisualState::Thinking).await;
        
        // 1. Trajectory inference (existing)
        self.maybe_evolve_persona().await;
        
        // 2. Journal consolidation
        self.consolidate_journal().await?;
        
        // 3. Concern maintenance
        let report = concerns.prune_and_consolidate().await?;
        if !report.demoted.is_empty() || !report.archived.is_empty() {
            tracing::info!(
                "Concern maintenance: {} demoted, {} archived",
                report.demoted.len(),
                report.archived.len()
            );
        }
        
        // 4. Memory evolution (existing ALMA-lite)
        self.maybe_run_memory_evolution().await;
        
        // 5. ALMA meta-agent (future)
        if self.config.read().await.enable_alma_exploration {
            self.run_alma_exploration().await?;
        }
        
        self.set_state(AgentVisualState::Idle).await;
        Ok(())
    }
}
```

---

### 8. Configuration Additions

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    // ... existing fields
    
    // Living Loop settings
    #[serde(default)]
    pub enable_ambient_loop: bool,
    
    #[serde(default = "default_ambient_min_interval")]
    pub ambient_min_interval_secs: u64,
    
    #[serde(default)]
    pub enable_journal: bool,
    
    #[serde(default = "default_journal_min_interval")]
    pub journal_min_interval_secs: u64,
    
    #[serde(default)]
    pub enable_concerns: bool,
    
    #[serde(default)]
    pub enable_dream_cycle: bool,
    
    #[serde(default = "default_dream_min_interval")]
    pub dream_min_interval_secs: u64,
    
    #[serde(default)]
    pub enable_alma_exploration: bool,
}

fn default_ambient_min_interval() -> u64 { 30 }
fn default_journal_min_interval() -> u64 { 300 }
fn default_dream_min_interval() -> u64 { 3600 }
```

---

### 9. Events and UI Integration

#### 9.1 New AgentEvents

```rust
pub enum AgentEvent {
    // ... existing variants
    
    OrientationUpdate(Orientation),
    JournalWritten(String),
    ConcernCreated(String, String),  // id, summary
    ConcernTouched(String),
    AttentionNeeded(String),
    DreamCycleStarted,
    DreamCycleCompleted,
}
```

#### 9.2 UI Considerations

- Orientation state could be shown as a subtle status indicator
- Journal entries could be viewable in a "Mind" or "Thoughts" panel
- Concerns could be listed in a dedicated panel
- Pending thoughts should surface naturally in chat greeting

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    // Presence
    #[test]
    fn test_time_context_calculations();
    #[test]
    fn test_process_categorization();
    
    // Orientation
    #[test]
    fn test_orientation_prompt_building();
    #[test]
    fn test_orientation_parsing();
    #[test]
    fn test_disposition_determination();
    
    // Journal
    #[test]
    fn test_journal_rate_limiting();
    #[test]
    fn test_journal_entry_parsing();
    
    // Concerns
    #[test]
    fn test_concern_salience_decay();
    #[test]
    fn test_concern_relationship_detection();
    #[test]
    fn test_concern_pruning();
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_ambient_loop_with_mock_llm();

#[tokio::test]
async fn test_disposition_execution();

#[tokio::test]
async fn test_dream_cycle_triggering();

#[tokio::test]
async fn test_journal_to_orientation_feedback();
```

---

## Migration Notes

### Backward Compatibility

- All new features disabled by default
- Existing heartbeat continues to work
- Database migrations are additive (new tables only)
- No changes to existing tool APIs

### Gradual Rollout

1. Deploy presence monitoring (no behavior change)
2. Deploy orientation engine (logged only)
3. Deploy journal system (opt-in)
4. Deploy concerns system (opt-in)
5. Enable loop split (config flag)
6. Enable dream cycle (config flag)

---

*This specification is a living document. Update as implementation proceeds.*
