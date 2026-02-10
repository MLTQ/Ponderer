mod agent;
mod character_card;
mod comfy_client;
mod comfy_workflow;
mod config;
mod database;
mod llm_client;
mod memory;
mod skills;
mod tools;
mod ui;

use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use agent::Agent;
use config::AgentConfig;
use database::AgentDatabase;
use skills::Skill;
use tools::ToolRegistry;
use ui::app::AgentApp;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,ponderer=debug")),
        )
        .init();

    tracing::info!("Ponderer starting...");

    // Load configuration from file (falls back to env vars if no file exists)
    let config = AgentConfig::load();

    tracing::info!("LLM: {} at {}", config.llm_model, config.llm_api_url);
    tracing::info!("Username: {}", config.username);
    if config.enable_self_reflection {
        tracing::info!(
            "Self-reflection enabled (interval: {}h)",
            config.reflection_interval_hours
        );
    }
    if config.enable_image_generation {
        tracing::info!(
            "Image generation enabled (ComfyUI: {})",
            config.comfyui.api_url
        );
    }

    tracing::info!("Tip: Make sure your LLM is running (e.g., `ollama serve` for Ollama)");

    // Build skills based on config
    let mut skill_list: Vec<Box<dyn Skill>> = Vec::new();

    // Add Graphchan skill if API URL is configured
    if !config.graphchan_api_url.is_empty() {
        tracing::info!("Graphchan skill enabled: {}", config.graphchan_api_url);
        skill_list.push(Box::new(skills::graphchan::GraphchanSkill::new(
            config.graphchan_api_url.clone(),
        )));
    }

    tracing::info!("Loaded {} skill(s)", skill_list.len());

    // Create tool registry and register built-in tools
    let tool_registry = Arc::new(ToolRegistry::new());
    {
        use tools::{
            files::{ListDirectoryTool, PatchFileTool, ReadFileTool, WriteFileTool},
            shell::ShellTool,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            tool_registry.register(Arc::new(ShellTool::new())).await;
            tool_registry.register(Arc::new(ReadFileTool::new())).await;
            tool_registry.register(Arc::new(WriteFileTool::new())).await;
            tool_registry
                .register(Arc::new(ListDirectoryTool::new()))
                .await;
            tool_registry.register(Arc::new(PatchFileTool::new())).await;
        });
    }
    tracing::info!("Tool registry initialized with 5 built-in tools");

    // Create event channel
    let (event_tx, event_rx) = flume::unbounded();

    // Create database for UI (shared with agent via same file, WAL mode allows concurrent access)
    let ui_database = match AgentDatabase::new(&config.database_path) {
        Ok(db) => Some(Arc::new(db)),
        Err(e) => {
            tracing::warn!("Failed to create UI database: {}", e);
            None
        }
    };

    // Create agent
    let agent = Arc::new(Agent::new(
        skill_list,
        tool_registry,
        config.clone(),
        event_tx,
    ));

    // Spawn agent loop in background
    let agent_clone = agent.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            if let Err(e) = agent_clone.run_loop().await {
                tracing::error!("Agent loop error: {}", e);
            }
        });
    });

    // Launch UI
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 800.0])
            .with_title("Ponderer"),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "Ponderer",
        native_options,
        Box::new(|_cc| {
            Ok(Box::new(AgentApp::new(
                event_rx,
                agent,
                config,
                ui_database,
            )))
        }),
    ) {
        tracing::error!("UI error: {}", e);
        std::process::exit(1);
    }
}
