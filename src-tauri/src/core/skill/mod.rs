pub mod registry;

use serde::{Deserialize, Serialize};

pub use registry::SkillRegistry;

/// A Skill is a bundle of tools plus a Markdown prompt body that teaches the
/// LLM how to combine them. The frontmatter (in `SKILL.md`) carries the
/// metadata below; the body is the prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub hitl: SkillHitl,
    /// Attachment slots this skill consumes. The frontend uses this to decide
    /// whether to surface the attachment button on the chat input. Currently
    /// recognised values: `"template"`. Unknown values are ignored.
    /// `alias` lets the SKILL.md author keep the YAML-natural snake_case form
    /// even though the struct itself is camelCase-renamed for the TS bridge.
    #[serde(default, alias = "accepts_attachments")]
    pub accepts_attachments: Vec<String>,
    #[serde(default = "default_language")]
    pub language: String,
    /// Prompt body (the Markdown below the frontmatter), used by the prompt
    /// composer when this skill is active on an agent.
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillHitl {
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub per_tool: std::collections::BTreeMap<String, bool>,
}

fn default_language() -> String {
    "en".to_string()
}
