use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::core::error::{CoreError, CoreResult};
use crate::core::skill::SkillRegistry;
use crate::core::tool::{Tool, ToolContext, ToolOutput, ToolSchema};

/// Tool that loads the full body of a SKILL.md on demand. The system prompt
/// only carries skill descriptions; the agent calls this when it decides a
/// skill is relevant, and the body lands in the conversation history as a
/// tool result.
#[derive(Debug, Clone)]
pub struct ReadSkillTool {
    skills: SkillRegistry,
}

impl ReadSkillTool {
    pub fn new(skills: SkillRegistry) -> Self {
        Self { skills }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    skill_id: String,
}

#[async_trait]
impl Tool for ReadSkillTool {
    fn name(&self) -> &'static str {
        "read_skill"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description:
                "Load the full instructions of one of the skills listed under \"Available \
                 skills\" in the system prompt. Call this only when the user's request maps to \
                 a specific skill — the body will arrive as the tool result and you must \
                 follow it. The list of valid `skillId` values is the `id` shown next to each \
                 skill title (e.g. `folder-search`, `document-extend`). Skills you have \
                 already loaded earlier in this conversation do not need to be re-loaded."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "skillId": {
                        "type": "string",
                        "description": "The id of the skill to load, exactly as listed in the system prompt."
                    }
                },
                "required": ["skillId"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, input: JsonValue, _ctx: &ToolContext) -> CoreResult<ToolOutput> {
        let parsed: Input = serde_json::from_value(input).map_err(CoreError::from)?;
        let skill = self.skills.get(&parsed.skill_id).ok_or_else(|| {
            CoreError::Llm(format!(
                "Unknown skill id `{}`. Valid ids are listed under \"Available skills\" in the system prompt.",
                parsed.skill_id
            ))
        })?;
        let body = if skill.body.trim().is_empty() {
            format!(
                "Skill `{}` has no body — the description in the system prompt is all there is.",
                skill.name
            )
        } else {
            skill.body.clone()
        };
        Ok(ToolOutput::text(body))
    }
}
