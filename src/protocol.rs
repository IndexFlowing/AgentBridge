use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// C2C conversation states. The Brain plans/reviews; the Executor writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum C2cState {
    Init,
    Plan,
    Executing,
    Executed,
    Review,
    Done,
    Blocked,
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
}
