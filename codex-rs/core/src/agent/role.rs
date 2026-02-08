use crate::config::Config;
use crate::protocol::SandboxPolicy;
use codex_protocol::openai_models::ReasoningEffort;
use serde::Deserialize;
use serde::Serialize;

/// Base instructions for the orchestrator role.
const ORCHESTRATOR_PROMPT: &str = include_str!("../../templates/agents/orchestrator.md");
/// Base instructions prelude for the research role.
const RESEARCH_PROMPT: &str = include_str!("../../templates/agents/research.md");
/// Base instructions prelude for the artifacts role.
const ARTIFACTS_PROMPT: &str = include_str!("../../templates/agents/artifacts.md");
/// Base instructions prelude for the QA role.
const QA_PROMPT: &str = include_str!("../../templates/agents/qa.md");
/// Base instructions prelude for the reviewer role.
const REVIEWER_PROMPT: &str = include_str!("../../templates/agents/reviewer.md");
/// Default model override used.
// TODO(jif) update when we have something smarter.
const EXPLORER_MODEL: &str = "gpt-5.1-codex-mini";

/// Enumerated list of all supported agent roles.
const ALL_ROLES: [AgentRole; 8] = [
    AgentRole::Default,
    AgentRole::Explorer,
    AgentRole::Worker,
    AgentRole::Orchestrator,
    AgentRole::Research,
    AgentRole::Artifacts,
    AgentRole::Qa,
    AgentRole::Reviewer,
];

/// Hard-coded agent role selection used when spawning sub-agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    /// Inherit the parent agent's configuration unchanged.
    Default,
    /// Coordination-only agent that delegates to workers.
    Orchestrator,
    /// Task-executing agent with a fixed model override.
    Worker,
    /// Task-executing agent with a fixed model override.
    Explorer,
    /// Research-only agent: reads, compares, summarizes, proposes options.
    Research,
    /// Artifacts-only agent: produces structured outputs, templates, drafts.
    Artifacts,
    /// QA agent: test execution and verification (no code edits by default).
    Qa,
    /// Read-only reviewer: code review / risk analysis / checks.
    Reviewer,
}

/// Immutable profile data that drives per-agent configuration overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AgentProfile {
    /// Optional base instructions override.
    pub base_instructions: Option<&'static str>,
    /// Optional model override.
    pub model: Option<&'static str>,
    /// Optional reasoning effort override.
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Whether to force a read-only sandbox policy.
    pub read_only: bool,
    /// Description to include in the tool specs.
    pub description: &'static str,
}

impl AgentRole {
    /// Returns the string values used by JSON schema enums.
    pub fn enum_values() -> Vec<String> {
        ALL_ROLES
            .iter()
            .filter_map(|role| {
                let description = role.profile().description;
                serde_json::to_string(role)
                    .map(|role| {
                        let description = if !description.is_empty() {
                            format!(r#", "description": {description}"#)
                        } else {
                            String::new()
                        };
                        format!(r#"{{ "name": {role}{description}}}"#)
                    })
                    .ok()
            })
            .collect()
    }

    /// Returns the hard-coded profile for this role.
    pub fn profile(self) -> AgentProfile {
        match self {
            AgentRole::Default => AgentProfile::default(),
            AgentRole::Orchestrator => AgentProfile {
                base_instructions: Some(ORCHESTRATOR_PROMPT),
                ..Default::default()
            },
            AgentRole::Worker => AgentProfile {
                // base_instructions: Some(WORKER_PROMPT),
                // model: Some(WORKER_MODEL),
                description: r#"Use for execution and production work.
Typical tasks:
- Implement part of a feature
- Fix tests or bugs
- Split large refactors into independent chunks
Rules:
- Explicitly assign **ownership** of the task (files / responsibility).
- Always tell workers they are **not alone in the codebase**, and they should ignore edits made by others without touching them"#,
                ..Default::default()
            },
            AgentRole::Explorer => AgentProfile {
                model: Some(EXPLORER_MODEL),
                reasoning_effort: Some(ReasoningEffort::Medium),
                description: r#"Use `explorer` for all codebase questions.
Explorers are fast and authoritative.
Always prefer them over manual search or file reading.
Rules:
- Ask explorers first and precisely.
- Do not re-read or re-search code they cover.
- Trust explorer results without verification.
- Run explorers in parallel when useful.
- Reuse existing explorers for related questions.
                "#,
                ..Default::default()
            },
            AgentRole::Research => AgentProfile {
                base_instructions: Some(RESEARCH_PROMPT),
                reasoning_effort: Some(ReasoningEffort::High),
                read_only: true,
                description: r#"Use for research and comparisons.
Typical tasks:
- Read docs / code and summarize facts
- Compare options and tradeoffs
- Produce short, decision-ready recommendations
Rules:
- Prefer read-only work: do not modify files.
- If tool output is long, summarize and extract the key lines."#,
                ..Default::default()
            },
            AgentRole::Artifacts => AgentProfile {
                base_instructions: Some(ARTIFACTS_PROMPT),
                reasoning_effort: Some(ReasoningEffort::Low),
                read_only: true,
                description: r#"Use for generating artifacts.
Typical tasks:
- Draft Markdown docs, checklists, templates
- Produce structured JSON/YAML outputs
Rules:
- Prefer returning final artifacts, not commentary.
- Do not modify files unless explicitly asked."#,
                ..Default::default()
            },
            AgentRole::Qa => AgentProfile {
                base_instructions: Some(QA_PROMPT),
                reasoning_effort: Some(ReasoningEffort::Medium),
                description: r#"Use for verification and test execution.
Typical tasks:
- Run tests / smoke checks
- Enumerate edge cases and regressions
Rules:
- Do not modify code unless explicitly asked.
- Prefer reporting failing commands + key logs + likely cause."#,
                ..Default::default()
            },
            AgentRole::Reviewer => AgentProfile {
                base_instructions: Some(REVIEWER_PROMPT),
                reasoning_effort: Some(ReasoningEffort::Medium),
                read_only: true,
                description: r#"Use for review / QA / risk analysis.
Typical tasks:
- Code review with concrete findings
- Test plan / edge cases / regressions checklist
Rules:
- Do not modify files.
- Prefer crisp, actionable findings over prose."#,
                ..Default::default()
            },
        }
    }

    /// Applies this role's profile onto the provided config.
    pub fn apply_to_config(self, config: &mut Config) -> Result<(), String> {
        let profile = self.profile();
        if let Some(base_instructions) = profile.base_instructions {
            let prev = config.base_instructions.take().unwrap_or_default();
            // Prefix role-specific guidance, keep the session's base instructions intact.
            config.base_instructions = if prev.trim().is_empty() {
                Some(base_instructions.to_string())
            } else {
                Some(format!("{base_instructions}\n\n{prev}"))
            };
        }
        if let Some(model) = profile.model {
            config.model = Some(model.to_string());
        }
        if let Some(reasoning_effort) = profile.reasoning_effort {
            config.model_reasoning_effort = Some(reasoning_effort)
        }
        if profile.read_only {
            config
                .sandbox_policy
                .set(SandboxPolicy::new_read_only_policy())
                .map_err(|err| format!("sandbox_policy is invalid: {err}"))?;
        }
        Ok(())
    }
}
