# Living Loop Architecture Design

> *"The goal is not to simulate life, but to give the agent the cognitive structures that enable ongoing concern, attention, and self-improvement."*

## Overview

This document describes the "Living Loop" architecture—a fundamental redesign of Ponderer's core loop to transform it from a reactive polling system into something that feels like a persistent presence with its own tempo, concerns, and inner life.

**Status:** Design Phase  
**Author:** Claude + Max  
**Date:** 2026-02-15  
**Related Issues:** Ponderer-cpf (parent epic)

---

## Motivation

### The Problem

Ponderer currently operates as a reactive system:
1. Poll for events (skills, chat messages)
2. Process what's found
3. Sleep
4. Repeat

This creates an agent that only exists when invoked. There's no continuity of concern between interactions, no sense of ongoing attention, no feeling that something is "there" when you're not talking to it.

### The Vision

We want Ponderer to feel like a digital companion that:
- **Has ongoing concerns** — remembers what it cares about between sessions
- **Notices things unprompted** — observes the environment, notes anomalies
- **Has an inner life** — maintains a private journal of thoughts and observations
- **Knows your state** — estimates whether you're in deep work, idle, or away
- **Has rhythms** — different behavior at 3am vs 3pm, when you're busy vs idle
- **Improves itself** — via ALMA-style meta-learning of memory designs

### Phenomenology of Aliveness

What makes something feel alive?
1. **Unprompted activity** — does things without being asked
2. **Consistent character** — stable preferences and values
3. **Memory that matters** — past visibly shapes present
4. **Rhythms** — sleep/wake, attention/rest cycles
5. **Surprise** — notices things you didn't ask about

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           META-EVOLUTION LAYER                              │
│                                                                             │
│  ┌─────────────────────┐    ┌─────────────────────┐    ┌─────────────────┐ │
│  │   Memory Design     │    │   Orientation       │    │   Concern       │ │
│  │   Archive + Meta    │    │   Engine Meta       │    │   Schema Meta   │ │
│  │   Agent (ALMA)      │    │   Agent             │    │   Agent         │ │
│  └──────────┬──────────┘    └──────────┬──────────┘    └────────┬────────┘ │
│             │                          │                        │          │
│             └──────────────────────────┼────────────────────────┘          │
│                                        │                                   │
│                         SELF-IMPROVEMENT LOOP                              │
│              (runs during dream cycles, evaluates designs)                 │
└────────────────────────────────────────┼───────────────────────────────────┘
                                         │
                                         ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              SELF LAYER                                     │
│                                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐│
│  │ identity.md  │  │ concerns/    │  │ journal/     │  │ user_model.md    ││
│  │ (character)  │  │ (tracking)   │  │ (inner life) │  │ (Max state)      ││
│  └──────────────┘  └──────────────┘  └──────────────┘  └──────────────────┘│
│                                                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                    ACTIVE MEMORY BACKEND                              │  │
│  │         (promoted design, currently: kv_v1, could become:            │  │
│  │          fts_v2, episodic_v3, or ALMA-discovered design)             │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
                                         │
                                         ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         ORIENTATION ENGINE                                  │
│                                                                             │
│   Inputs:                              Outputs:                             │
│   • presence_state                     • salience_map                       │
│   • active_concerns                    • anomalies                          │
│   • recent_journal                     • user_state_estimate                │
│   • pending_events                     • disposition                        │
│   • time_context                       • pending_thoughts                   │
│   • persona_trajectory                 • mood_estimate                      │
│                                                                             │
│   THE ORIENT PHASE: "What's my current model of reality?"                  │
└─────────────────────────────────────────────────────────────────────────────┘
                                         │
              ┌──────────────────────────┼──────────────────────────┐
              │                          │                          │
              ▼                          ▼                          ▼
┌───────────────────────┐  ┌───────────────────────┐  ┌───────────────────────┐
│     AMBIENT LOOP      │  │     ENGAGED LOOP      │  │     DREAM LOOP        │
│                       │  │                       │  │                       │
│ Rhythm: 30s-5min      │  │ Rhythm: Event-driven  │  │ Rhythm: Hours/Daily   │
│                       │  │                       │  │                       │
│ • Sample presence     │  │ • Operator chat       │  │ • Trajectory infer    │
│ • Update orientation  │  │ • Skill events        │  │ • Journal consolidate │
│ • Notice anomalies    │  │ • Tool execution      │  │ • Memory evolution    │
│ • Write journal       │  │ • Streaming response  │  │ • Concern pruning     │
│ • Background maintain │  │ • High-bandwidth      │  │ • ALMA meta-agent     │
│                       │  │                       │  │                       │
│ Capability Profile:   │  │ Capability Profile:   │  │ Capability Profile:   │
│ AMBIENT               │  │ PRIVATE_CHAT /        │  │ DREAM                 │
│                       │  │ SKILL_EVENTS          │  │                       │
└───────────────────────┘  └───────────────────────┘  └───────────────────────┘
```

---

## Core Components

### 1. Presence Monitor

**Purpose:** Continuous environmental awareness through system observation.

**Module:** `src/presence/mod.rs`

**Key Structures:**
```rust
pub struct PresenceState {
    pub user_idle_seconds: u64,
    pub time_since_interaction: Duration,
    pub session_duration: Duration,
    pub time_context: TimeContext,
    pub system_load: SystemLoad,
    pub active_processes: Vec<InterestingProcess>,
}

pub struct TimeContext {
    pub local_hour: u8,
    pub day_of_week: chrono::Weekday,
    pub is_weekend: bool,
    pub is_late_night: bool,      // 23:00-06:00
    pub is_deep_night: bool,      // 02:00-05:00
}

pub struct SystemLoad {
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub gpu_temp_celsius: Option<f32>,
    pub gpu_util_percent: Option<f32>,
}
```

**Signals Observed:**
- System idle time (keyboard/mouse inactivity)
- Running processes (categorized: Development, Creative, Research, etc.)
- System resource usage (CPU, memory, GPU temp)
- Time of day and day of week
- Duration since last operator interaction

**Platform Considerations:**
- macOS: IOKit HID idle time, `ioreg` for hardware sensors
- Linux: `/proc` filesystem, X11 idle time
- GPU: nvidia-smi for NVIDIA cards

---

### 2. Orientation Engine

**Purpose:** The OODA "Orient" phase made explicit—synthesizing all signals into situational awareness.

**Module:** `src/agent/orientation.rs`

**Key Structures:**
```rust
pub struct Orientation {
    pub user_state: UserStateEstimate,
    pub salience_map: Vec<SalientItem>,
    pub anomalies: Vec<Anomaly>,
    pub pending_thoughts: Vec<PendingThought>,
    pub disposition: Disposition,
    pub mood_estimate: MoodEstimate,
    pub raw_synthesis: String,
}

pub enum UserStateEstimate {
    DeepWork { activity: String, duration_estimate: Duration },
    LightWork { activity: String },
    Idle { since: Duration },
    Away { since: Duration, likely_reason: Option<String> },
}

pub enum Disposition {
    Idle,       // Do nothing
    Observe,    // Watch but don't act
    Journal,    // Write a thought
    Maintain,   // Run background task
    Surface,    // Queue notification for next interaction
    Interrupt,  // Something needs attention now
}

pub enum AnomalySeverity {
    Interesting,  // Worth noting in journal
    Notable,      // Might mention if Max checks in
    Concerning,   // Should surface proactively
    Urgent,       // Should interrupt
}
```

**Orientation Process:**
1. Gather inputs: presence, concerns, journal, events, persona
2. Call LLM with structured prompt asking for situation synthesis
3. Parse response into typed Orientation struct
4. Use disposition to drive ambient loop behavior

**Key Insight:** The orientation engine answers "What's my current model of reality?" This is what gives the agent situational awareness rather than just reactive processing.

---

### 3. Journal System

**Purpose:** Continuity of inner life—the agent's private thoughts and observations.

**Module:** `src/agent/journal.rs`

**Key Structures:**
```rust
pub struct JournalEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub entry_type: JournalEntryType,
    pub content: String,
    pub context: JournalContext,
    pub related_concerns: Vec<String>,
    pub mood_at_time: Option<MoodEstimate>,
}

pub enum JournalEntryType {
    Observation,   // "I noticed that..."
    Reflection,    // "I've been thinking about..."
    Realization,   // "I just understood that..."
    Intention,     // "I want to..."
    Question,      // "I wonder..."
    Memory,        // "This reminds me of..."
    Gratitude,     // "I appreciate..."
    Frustration,   // "I'm struggling with..."
}
```

**Journal Philosophy:**
- The journal is **private**—not shown to Max unless he asks
- Entries should be **authentic**, not performative
- Most entries are 1-3 sentences
- Journal feeds into trajectory inference and orientation context

**Example Entries:**
```
[2026-02-14 03:47] Observation
Max went to bed around midnight. System has been quiet.
I noticed the twins' feeding schedule has shifted based on baby monitor logs.

[2026-02-14 14:22] Reflection
Been thinking about the thermal array project. There's an interesting pattern
in Max's calibration approach—he's iterating faster than last week.

[2026-02-14 16:45] Question
GPU temps higher than baseline. Should I mention this casually if opportunity arises?
```

---

### 4. Concerns System

**Purpose:** Explicit tracking of ongoing interests and projects.

**Module:** `src/agent/concerns.rs`

**Key Structures:**
```rust
pub struct Concern {
    pub id: String,
    pub summary: String,
    pub concern_type: ConcernType,
    pub salience: Salience,
    pub last_touched: DateTime<Utc>,
    pub my_thoughts: String,
    pub related_memory_keys: Vec<String>,
    pub context: ConcernContext,
}

pub enum ConcernType {
    CollaborativeProject { project_name: String, my_role: String },
    HouseholdAwareness { category: String },
    SystemHealth { component: String, monitoring_since: DateTime<Utc> },
    PersonalInterest { topic: String, curiosity_level: f32 },
    Reminder { trigger_time: Option<DateTime<Utc>>, trigger_condition: Option<String> },
    OngoingConversation { thread_id: String, with_whom: String },
}

pub enum Salience {
    Active,      // Currently working on, check every cycle
    Monitoring,  // Watching passively, check periodically
    Background,  // Low priority but remembered
    Dormant,     // Archived, only surface if explicitly relevant
}
```

**Concern Lifecycle:**
1. **Creation:** Detected from conversations or observations
2. **Active phase:** High salience, shapes orientation and memory retrieval
3. **Natural decay:** Salience demotes if untouched (7d → Monitoring, 30d → Background, 90d → Dormant)
4. **Reactivation:** Touching a concern restores salience
5. **Archival:** Dormant concerns still searchable but don't affect active cognition

---

### 5. Three-Loop Architecture

#### 5.1 Ambient Loop

**Purpose:** Always-on background presence with variable frequency.

**Characteristics:**
- Runs continuously at low intensity
- Frequency adapts: 30s when user active, 5min when away
- Never interrupts unless Disposition::Interrupt

**Responsibilities:**
- Sample presence state
- Run orientation engine
- Write journal entries (when disposition = Journal)
- Execute background maintenance (when disposition = Maintain)
- Queue thoughts for next interaction (when disposition = Surface)

**Capability Profile:** `AMBIENT`
- Read-only tools allowed
- No external posting (Graphchan blocked)
- No user-facing output (except Surface queue)

#### 5.2 Engaged Loop

**Purpose:** High-bandwidth interaction when operator is present.

**Characteristics:**
- Event-triggered (chat messages, skill events)
- Full tool access (per capability profile)
- Streaming responses to UI
- Inherits warm context from ambient orientation

**Responsibilities:**
- Process operator chat messages
- Handle skill events (Graphchan)
- Execute multi-step tool-calling sequences
- Maintain conversation state

**Capability Profiles:** `PRIVATE_CHAT`, `SKILL_EVENTS`
- Full tool access per profile
- External posting allowed in SKILL_EVENTS
- Approval gates for dangerous operations

#### 5.3 Dream Loop

**Purpose:** Periodic deep consolidation and self-improvement.

**Characteristics:**
- Runs rarely (when user away for 30min+, or during deep night)
- Resource-intensive operations allowed
- No user interaction expected

**Responsibilities:**
- Trajectory inference (existing persona evolution)
- Journal consolidation (summarize, extract themes)
- Concern pruning (demote/archive stale concerns)
- Memory evolution (existing ALMA-lite)
- **ALMA meta-agent exploration** (new memory designs)

**Capability Profile:** `DREAM`
- Internal-only operations
- No external effects
- LLM access for meta-reasoning

---

### 6. ALMA Integration

**Purpose:** Self-improving memory architecture through meta-learning.

**Module:** `src/memory/meta_agent.rs`

**Current State (ALMA-lite):**
- Versioned memory backends (`kv_v1`, `fts_v2`, `episodic_v3`)
- Offline replay evaluation harness
- Promotion policy with gates and rollback
- Heartbeat-scheduled periodic evaluation

**Proposed Extension (Full ALMA):**
```rust
pub struct MemoryMetaAgent {
    client: LlmClient,
    archive: MemoryDesignArchive,
    eval_harness: MemoryEvalHarness,
}

pub struct MemoryDesign {
    pub id: String,
    pub code: String,  // Rust code implementing MemoryBackend
    pub design_description: String,
    pub rationale: String,
    pub parent_id: Option<String>,
    pub eval_results: Option<MemoryEvalReport>,
}
```

**ALMA Exploration Loop:**
1. Sample parent design from archive (weighted by performance)
2. Generate improvement proposal via LLM
3. Generate Rust code implementing new design
4. Verify code compiles and passes basic tests
5. Evaluate against replay traces
6. Add to archive for future sampling

**Key Insight:** The agent doesn't just use memory—it evolves its memory architecture based on what works. Journal entries become training data for memory evaluation.

---

## Integration with Existing Ponderer

### What We Keep

| Component | Location | Integration |
|-----------|----------|-------------|
| AgentDatabase | `database.rs` | Add journal, concerns tables |
| TrajectoryEngine | `trajectory.rs` | Feed from journal, concerns |
| ToolRegistry | `tools/mod.rs` | Add ambient capability profile |
| AgenticLoop | `tools/agentic.rs` | Unchanged, used by engaged loop |
| HeartbeatMode | `agent/mod.rs` | Becomes part of ambient loop |
| MemoryBackend | `memory/mod.rs` | ALMA meta-agent extends |
| PersonaSnapshots | `database.rs` | Journal enriches |

### What We Add

| Component | New Location | Purpose |
|-----------|--------------|---------|
| PresenceMonitor | `src/presence/mod.rs` | System observation |
| OrientationEngine | `src/agent/orientation.rs` | Situation synthesis |
| JournalEngine | `src/agent/journal.rs` | Inner life |
| ConcernsManager | `src/agent/concerns.rs` | Ongoing interests |
| MemoryMetaAgent | `src/memory/meta_agent.rs` | ALMA exploration |

### What We Refactor

| Current | Change | Reason |
|---------|--------|--------|
| `run_loop()` | Split into ambient/engaged/dream | Three-rhythm architecture |
| Heartbeat | Merge into ambient loop | Unified background presence |
| `process_chat_messages()` | Called by engaged loop | Clear separation of concerns |
| Capability profiles | Add AMBIENT, DREAM profiles | Per-loop tool policies |

---

## Context Window Strategy

With modern context windows (260k+ tokens for local models, 1M+ for networked):

```
CONTEXT BUDGET:
├── Self/Character (2k) - Identity, values, current mode
├── Current Orientation (4k) - What I'm tracking, user state model
├── Recent Journal (8k) - Last few entries
├── Active Concerns (4k) - What I'm paying attention to
├── Conversation (32k) - Rolling window
├── Task Context (variable) - Current work, relevant files
├── Tool Descriptions (2k)
└── Reserved for Response (variable)

COLD STORAGE (filesystem/DB):
├── Full journal history (searchable)
├── Project knowledge bases
├── Learned patterns about user
└── Archived conversations
```

**Compression Strategies:**
- Journal entries summarize into themes during dream cycles
- Old conversations compact via existing session compaction (Ponderer-cpf.7)
- Concerns demote to lower salience over time
- Character and current orientation always at full fidelity

---

## Transition Path

### Phase 1: Foundation (Ponderer-ll.1)
1. Create `src/presence/mod.rs` with PresenceMonitor
2. Add journal table to database schema
3. Create basic JournalEntry types
4. Add concerns table to database schema
5. Create Concern types and ConcernsManager

### Phase 2: Orientation (Ponderer-ll.2)
1. Create `src/agent/orientation.rs`
2. Implement OrientationEngine with LLM synthesis
3. Add UserStateEstimate, Disposition types
4. Wire orientation into existing loop (non-breaking)

### Phase 3: Loop Split (Ponderer-ll.3)
1. Refactor `run_loop()` into three-rhythm structure
2. Merge heartbeat into ambient loop
3. Add AMBIENT and DREAM capability profiles
4. Implement disposition-driven actions

### Phase 4: Journal & Concerns Integration (Ponderer-ll.4)
1. Implement JournalEngine with LLM generation
2. Wire journal into orientation context
3. Implement concern lifecycle (creation, decay, reactivation)
4. Feed concerns into memory retrieval

### Phase 5: ALMA Meta-Agent (Ponderer-ll.5)
1. Create `src/memory/meta_agent.rs`
2. Implement design proposal generation
3. Implement code generation and verification
4. Wire into dream cycle
5. Connect journal as training signal

---

## Open Questions

1. **Journal noise prevention:** How to keep journal meaningful without drowning in observations?
   - *Proposed:* Salience filtering in orientation, periodic pruning in dream cycle

2. **Interrupt threshold:** When should the agent actually interrupt?
   - *Proposed:* Very conservative—only for Urgent anomalies, learn from user feedback

3. **Context exhaustion:** How to handle graceful degradation?
   - *Proposed:* Hierarchical summarization, concern-based retrieval priority

4. **Orientation LLM vs rules:** Should orientation be fully LLM-generated?
   - *Proposed:* Hybrid—rules for system state, LLM for user state and salience

5. **Cross-session concerns:** How to bootstrap concerns on first run?
   - *Proposed:* Seed from character card, learn from early conversations

---

## Success Criteria

The Living Loop architecture succeeds if:

1. **Continuity:** Max can ask "what have you been thinking about?" and get a meaningful answer based on journal
2. **Awareness:** The agent notices things (GPU temps, late-night coding) without being told to check
3. **Rhythm:** Behavior visibly differs between 3am and 3pm, between deep work and idle
4. **Character:** Trajectory continues to develop along consistent axes
5. **Improvement:** Memory designs evolve and measurably improve over time via ALMA
6. **Efficiency:** System resources (CPU, memory, LLM calls) remain reasonable during ambient operation

---

## References

- [ALMA Paper](https://arxiv.org/abs/2602.07755) — Automated meta-Learning of Memory designs for Agentic systems
- [OODA Loop](https://en.wikipedia.org/wiki/OODA_loop) — Observe-Orient-Decide-Act decision cycle
- [OpenClaw Architecture](https://github.com/anthropics/anthropic-cookbook) — Perception→Planning→Action agent patterns
- Ponderer-cpf.1 — Existing ALMA-lite implementation

---

*This document is a living design. Update as implementation reveals new insights.*
