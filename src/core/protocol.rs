use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// C2C conversation states. The Brain plans/reviews; the Executor writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] // 不需要 #[default] 宏了
#[serde(rename_all = "UPPERCASE")]
pub enum C2cState {
    Init,
    Plan,
    Executing,
    Executed,
    Review,
    Done,
    Blocked,
    Cancelled,
}

// 直接手写一个极简的 Default 实现，绝对不会报错！
impl Default for C2cState {
    fn default() -> Self {
        Self::Init
    }
}

impl C2cState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Init => "INIT",
            Self::Plan => "PLAN",
            Self::Executing => "EXECUTING",
            Self::Executed => "EXECUTED",
            Self::Review => "REVIEW",
            Self::Done => "DONE",
            Self::Blocked => "BLOCKED",
            Self::Cancelled => "CANCELLED",
        }
    }
}

impl Display for C2cState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for C2cState {
    type Err = ProtocolError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "INIT" => Ok(Self::Init),
            "PLAN" => Ok(Self::Plan),
            "EXECUTING" => Ok(Self::Executing),
            "EXECUTED" => Ok(Self::Executed),
            "REVIEW" => Ok(Self::Review),
            "DONE" => Ok(Self::Done),
            "BLOCKED" => Ok(Self::Blocked),
            "CANCELLED" => Ok(Self::Cancelled),
            other => Err(ProtocolError::InvalidState(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Recommendation {
    Done,
    Plan,
    Blocked,
}

impl FromStr for Recommendation {
    type Err = ProtocolError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "DONE" => Ok(Self::Done),
            "PLAN" => Ok(Self::Plan),
            "BLOCKED" => Ok(Self::Blocked),
            other => Err(ProtocolError::InvalidRecommendation(other.to_string())),
        }
    }
}

impl Display for Recommendation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Done => f.write_str("DONE"),
            Self::Plan => f.write_str("PLAN"),
            Self::Blocked => f.write_str("BLOCKED"),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("C2C message must start with [C2C]")]
    MissingHeader,
    #[error("C2C message is missing STATE")]
    MissingState,
    #[error("invalid C2C state: {0}")]
    InvalidState(String),
    #[error("invalid C2C recommendation: {0}")]
    InvalidRecommendation(String),
    #[error("invalid ITERATION value: {0}")]
    InvalidIteration(String),
    #[error("C2C PLAN is missing a goal")]
    MissingGoal,
    #[error("C2C PLAN must include at least one action")]
    MissingActions,
    #[error("C2C PLAN is missing success criteria")]
    MissingSuccessCriteria,
    #[error("C2C PLAN field {0} is too large")]
    FieldTooLarge(&'static str),
    #[error("C2C PLAN {0}")]
    InvalidPlan(String),
}

const MAX_GOAL_CHARS: usize = 8_000;
const MAX_ACTION_CHARS: usize = 2_000;
const MAX_ACTIONS: usize = 40;
const MAX_TEST_CHARS: usize = 500;
const MAX_TESTS: usize = 16;
const MAX_CRITERIA_CHARS: usize = 4_000;

/// Structured Brain → Executor PLAN. Source code and diffs stay in MCP, not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct C2cPlan {
    pub task_id: String,
    pub iteration: u32,
    pub goal: String,
    pub actions: Vec<String>,
    pub tests: Vec<String>,
    pub success_criteria: String,
}

impl C2cPlan {
    pub fn new(
        task_id: String,
        iteration: u32,
        goal: String,
        actions: Vec<String>,
        tests: Vec<String>,
        success_criteria: String,
    ) -> Result<Self, ProtocolError> {
        let plan = Self {
            task_id,
            iteration,
            goal,
            actions,
            tests,
            success_criteria,
        };
        plan.validate()?;
        Ok(plan)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.task_id.trim().is_empty() {
            return Err(ProtocolError::InvalidPlan("task_id is required".into()));
        }
        if self.iteration == 0 {
            return Err(ProtocolError::InvalidPlan("iteration must be >= 1".into()));
        }
        let goal = self.goal.trim();
        if goal.is_empty() {
            return Err(ProtocolError::MissingGoal);
        }
        if goal.len() > MAX_GOAL_CHARS {
            return Err(ProtocolError::FieldTooLarge("GOAL"));
        }
        let actions: Vec<&str> = self
            .actions
            .iter()
            .map(|a| a.trim())
            .filter(|a| !a.is_empty())
            .collect();
        if actions.is_empty() {
            return Err(ProtocolError::MissingActions);
        }
        if actions.len() > MAX_ACTIONS {
            return Err(ProtocolError::FieldTooLarge("ACTIONS"));
        }
        if actions.iter().any(|a| a.len() > MAX_ACTION_CHARS) {
            return Err(ProtocolError::FieldTooLarge("ACTIONS"));
        }
        if self.tests.len() > MAX_TESTS {
            return Err(ProtocolError::FieldTooLarge("TESTS"));
        }
        if self.tests.iter().any(|t| t.len() > MAX_TEST_CHARS) {
            return Err(ProtocolError::FieldTooLarge("TESTS"));
        }
        let criteria = self.success_criteria.trim();
        if criteria.is_empty() {
            return Err(ProtocolError::MissingSuccessCriteria);
        }
        if criteria.len() > MAX_CRITERIA_CHARS {
            return Err(ProtocolError::FieldTooLarge("SUCCESS_CRITERIA"));
        }
        Ok(())
    }

    pub fn tests_command(&self) -> Option<String> {
        let joined = self
            .tests
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if joined.is_empty() {
            None
        } else {
            Some(joined)
        }
    }

    pub fn to_message(&self) -> C2cMessage {
        C2cMessage {
            state: Some(C2cState::Plan),
            task_id: Some(self.task_id.clone()),
            iteration: Some(self.iteration),
            goal: Some(self.goal.trim().to_string()),
            actions: Some(render_actions(&self.actions)),
            tests: self.tests_command(),
            success_criteria: Some(self.success_criteria.trim().to_string()),
            ..Default::default()
        }
    }

    /// Prompt given to OpenCode. Compact PLAN only — no source dumps.
    pub fn to_executor_prompt(&self) -> String {
        let plan_text = self.to_message().render();
        format!(
            "You are the Executor for AgentBridge.\n\
             \n\
             Implement the PLAN below in the current working directory only.\n\
             Do not modify files outside this directory.\n\
             Do not expand scope.\n\
             Do not print chain-of-thought or internal reasoning.\n\
             Inspect, edit files, run the listed tests, then stop.\n\
             \n\
             When finished, print a short summary with:\n\
             - changed file names only\n\
             - test command and outcome\n\
             - overall success or failure\n\
             Do not paste entire source files.\n\
             \n\
             {plan_text}"
        )
    }
}

fn render_actions(actions: &[String]) -> String {
    actions
        .iter()
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
        .enumerate()
        .map(|(i, a)| {
            if looks_numbered(a) {
                a.to_string()
            } else {
                format!("{}. {a}", i + 1)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn looks_numbered(action: &str) -> bool {
    let bytes = action.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return false;
    }
    matches!(
        action.get(i..),
        Some(rest) if rest.starts_with(". ") || rest.starts_with(") ") || rest.starts_with('.')
    )
}

/// A compact Brain ↔ Executor message. Do not embed source files or huge diffs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct C2cMessage {
    pub state: Option<C2cState>,
    pub task_id: Option<String>,
    pub iteration: Option<u32>,
    pub goal: Option<String>,
    pub actions: Option<String>,
    pub tests: Option<String>,
    pub success_criteria: Option<String>,
    pub status: Option<String>,
    pub changed_files: Option<String>,
    pub result: Option<String>,
    pub recommendation: Option<Recommendation>,
    pub notes: Option<String>,
}

impl C2cMessage {
    pub fn parse(input: &str) -> Result<Self, ProtocolError> {
        let text = input.trim();
        let mut lines = text.lines();
        let first = lines.next().map(str::trim).unwrap_or("");
        if !first.eq_ignore_ascii_case("[C2C]") {
            return Err(ProtocolError::MissingHeader);
        }

        let rest: Vec<&str> = lines.collect();
        let mut msg = C2cMessage::default();
        let mut i = 0;
        while i < rest.len() {
            let line = rest[i];
            if line.trim().is_empty() {
                i += 1;
                continue;
            }
            if let Some((key, value)) = split_key(line) {
                let (body, next) = if value.is_empty() {
                    take_block(&rest, i + 1)
                } else {
                    (value.to_string(), i + 1)
                };
                assign_field(&mut msg, &key, body)?;
                i = next;
            } else {
                let notes = msg.notes.get_or_insert_with(String::new);
                if !notes.is_empty() {
                    notes.push('\n');
                }
                notes.push_str(line);
                i += 1;
            }
        }

        if msg.state.is_none() {
            return Err(ProtocolError::MissingState);
        }
        Ok(msg)
    }

    pub fn render(&self) -> String {
        let mut out = String::from("[C2C]\n");
        if let Some(state) = self.state {
            out.push_str(&format!("STATE: {state}\n"));
        }
        if let Some(id) = &self.task_id {
            out.push_str(&format!("TASK_ID: {id}\n"));
        }
        if let Some(n) = self.iteration {
            out.push_str(&format!("ITERATION: {n}\n"));
        }
        push_block(&mut out, "GOAL", self.goal.as_deref());
        push_block(&mut out, "ACTIONS", self.actions.as_deref());
        push_block(&mut out, "TESTS", self.tests.as_deref());
        push_block(
            &mut out,
            "SUCCESS_CRITERIA",
            self.success_criteria.as_deref(),
        );
        if let Some(status) = &self.status {
            out.push_str(&format!("\nSTATUS: {status}\n"));
        }
        push_block(&mut out, "CHANGED_FILES", self.changed_files.as_deref());
        push_block(&mut out, "RESULT", self.result.as_deref());
        if let Some(rec) = self.recommendation {
            out.push_str(&format!("\nRECOMMENDATION:\n{rec}\n"));
        }
        if let Some(notes) = &self.notes {
            if !notes.is_empty() {
                out.push('\n');
                out.push_str(notes);
                if !notes.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
        out
    }
}

fn split_key(line: &str) -> Option<(String, String)> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || !is_field_key(key) {
        return None;
    }
    Some((key.to_ascii_uppercase(), value.trim().to_string()))
}

fn is_field_key(key: &str) -> bool {
    matches!(
        key.to_ascii_uppercase().as_str(),
        "STATE"
            | "TASK_ID"
            | "ITERATION"
            | "GOAL"
            | "ACTIONS"
            | "TESTS"
            | "SUCCESS_CRITERIA"
            | "STATUS"
            | "CHANGED_FILES"
            | "RESULT"
            | "RECOMMENDATION"
    )
}

fn take_block(lines: &[&str], start: usize) -> (String, usize) {
    let mut i = start;
    let mut body = Vec::new();
    while i < lines.len() {
        if lines[i].trim().is_empty() || split_key(lines[i]).is_some() {
            break;
        }
        body.push(lines[i]);
        i += 1;
    }
    (body.join("\n").trim().to_string(), i)
}

fn assign_field(msg: &mut C2cMessage, key: &str, body: String) -> Result<(), ProtocolError> {
    match key {
        "STATE" => msg.state = Some(body.parse()?),
        "TASK_ID" => msg.task_id = Some(body),
        "ITERATION" => {
            let n = body
                .trim()
                .parse::<u32>()
                .map_err(|_| ProtocolError::InvalidIteration(body.clone()))?;
            msg.iteration = Some(n);
        }
        "GOAL" => msg.goal = Some(body),
        "ACTIONS" => msg.actions = Some(body),
        "TESTS" => msg.tests = Some(body),
        "SUCCESS_CRITERIA" => msg.success_criteria = Some(body),
        "STATUS" => msg.status = Some(body),
        "CHANGED_FILES" => msg.changed_files = Some(body),
        "RESULT" => msg.result = Some(body),
        "RECOMMENDATION" => {
            // First token is the recommendation; ignore trailing commentary.
            let token = body
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or(&body)
                .split_whitespace()
                .next()
                .unwrap_or(&body);
            msg.recommendation = Some(token.parse()?);
        }
        _ => {}
    }
    Ok(())
}

fn push_block(out: &mut String, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        out.push('\n');
        out.push_str(key);
        out.push_str(":\n");
        out.push_str(value);
        if !value.ends_with('\n') {
            out.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_state(text: &str, expected: C2cState) {
        let msg = C2cMessage::parse(text).unwrap();
        assert_eq!(msg.state, Some(expected));
    }

    #[test]
    fn valid_states() {
        for state in [
            C2cState::Init,
            C2cState::Plan,
            C2cState::Executing,
            C2cState::Executed,
            C2cState::Review,
            C2cState::Done,
            C2cState::Blocked,
            C2cState::Cancelled,
        ] {
            let text = format!("[C2C]\nSTATE: {state}\nTASK_ID: t1\n");
            assert_state(&text, state);
        }
    }

    #[test]
    fn parse_plan_example() {
        let text = r#"
[C2C]
STATE: PLAN
TASK_ID: c2c_12345
ITERATION: 1

GOAL:
Implement Google Search Console URL inspection.

ACTIONS:
1. Inspect current GSC integration.
2. Add URL inspection client.
3. Add database persistence.
4. Add tests.

TESTS:
cargo test

SUCCESS_CRITERIA:
All tests pass and the API returns indexed/not-indexed status.
"#;
        let msg = C2cMessage::parse(text).unwrap();
        assert_eq!(msg.state, Some(C2cState::Plan));
        assert_eq!(msg.task_id.as_deref(), Some("c2c_12345"));
        assert_eq!(msg.iteration, Some(1));
        assert!(msg.goal.as_deref().unwrap().contains("URL inspection"));
        assert!(msg.actions.as_deref().unwrap().contains("Add tests"));
        assert_eq!(msg.tests.as_deref(), Some("cargo test"));
    }

    #[test]
    fn parse_executed_example() {
        let text = r#"
[C2C]
STATE: EXECUTED
TASK_ID: c2c_12345
ITERATION: 1

STATUS: SUCCESS

CHANGED_FILES:
3

TESTS:
cargo test
42 passed

Please inspect the current workspace and git diff through MCP.
"#;
        let msg = C2cMessage::parse(text).unwrap();
        assert_eq!(msg.state, Some(C2cState::Executed));
        assert_eq!(msg.status.as_deref(), Some("SUCCESS"));
        assert!(msg.tests.as_deref().unwrap().contains("42 passed"));
        assert!(msg
            .notes
            .as_deref()
            .unwrap()
            .contains("inspect the current workspace"));
    }

    #[test]
    fn parse_review_example() {
        let text = r#"
[C2C]
STATE: REVIEW
TASK_ID: c2c_12345
ITERATION: 1

RESULT:
CHANGES_LOOK_GOOD

RECOMMENDATION:
DONE
"#;
        let msg = C2cMessage::parse(text).unwrap();
        assert_eq!(msg.state, Some(C2cState::Review));
        assert_eq!(msg.recommendation, Some(Recommendation::Done));
        assert_eq!(msg.result.as_deref(), Some("CHANGES_LOOK_GOOD"));
    }

    #[test]
    fn invalid_states_rejected() {
        for bad in ["RUNNING", "FOO", "complete", ""] {
            let text = format!("[C2C]\nSTATE: {bad}\n");
            let err = C2cMessage::parse(&text).unwrap_err();
            assert!(
                matches!(
                    err,
                    ProtocolError::InvalidState(_) | ProtocolError::MissingState
                ),
                "expected invalid state for {bad:?}, got {err:?}"
            );
        }
    }

    #[test]
    fn missing_header_rejected() {
        let err = C2cMessage::parse("STATE: PLAN\n").unwrap_err();
        assert_eq!(err, ProtocolError::MissingHeader);
    }

    #[test]
    fn missing_state_rejected() {
        let err = C2cMessage::parse("[C2C]\nTASK_ID: x\n").unwrap_err();
        assert_eq!(err, ProtocolError::MissingState);
    }

    #[test]
    fn render_roundtrip() {
        let original = C2cMessage {
            state: Some(C2cState::Plan),
            task_id: Some("c2c_1".into()),
            iteration: Some(2),
            goal: Some("Do the thing".into()),
            tests: Some("cargo test".into()),
            ..Default::default()
        };
        let rendered = original.render();
        let parsed = C2cMessage::parse(&rendered).unwrap();
        assert_eq!(parsed.state, original.state);
        assert_eq!(parsed.task_id, original.task_id);
        assert_eq!(parsed.iteration, original.iteration);
        assert_eq!(parsed.goal, original.goal);
        assert_eq!(parsed.tests, original.tests);
    }

    #[test]
    fn plan_requires_goal_actions_criteria() {
        let err = C2cPlan::new(
            "c2c_1".into(),
            1,
            " ".into(),
            vec!["do it".into()],
            vec![],
            "tests pass".into(),
        )
        .unwrap_err();
        assert_eq!(err, ProtocolError::MissingGoal);

        let err = C2cPlan::new(
            "c2c_1".into(),
            1,
            "goal".into(),
            vec!["  ".into()],
            vec![],
            "tests pass".into(),
        )
        .unwrap_err();
        assert_eq!(err, ProtocolError::MissingActions);

        let err = C2cPlan::new(
            "c2c_1".into(),
            1,
            "goal".into(),
            vec!["do it".into()],
            vec![],
            " ".into(),
        )
        .unwrap_err();
        assert_eq!(err, ProtocolError::MissingSuccessCriteria);
    }

    #[test]
    fn plan_renders_c2c_and_executor_prompt() {
        let plan = C2cPlan::new(
            "c2c_12345".into(),
            1,
            "Fix the sitemap parser performance issue.".into(),
            vec![
                "Inspect the current sitemap parser.".into(),
                "Identify unnecessary allocations.".into(),
                "Implement the optimization.".into(),
                "Add or update tests.".into(),
            ],
            vec!["cargo test".into()],
            "All tests pass and parser behavior remains unchanged.".into(),
        )
        .unwrap();
        let rendered = plan.to_message().render();
        assert!(rendered.contains("STATE: PLAN"));
        assert!(rendered.contains("TASK_ID: c2c_12345"));
        assert!(rendered.contains("1. Inspect the current sitemap parser."));
        assert!(rendered.contains("TESTS:\ncargo test"));
        let prompt = plan.to_executor_prompt();
        assert!(prompt.contains("You are the Executor for AgentBridge."));
        assert!(prompt.contains("current working directory only"));
        assert!(!prompt.contains("fn main"));
    }
}
