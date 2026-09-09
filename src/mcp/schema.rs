use std::fmt;
use rmcp::schemars;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PathArgs {
    #[serde(default = "default_dot")]
    pub path: String,
    #[serde(default)]
    pub project: Option<String>,
}

fn default_dot() -> String {
    ".".to_string()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadFileArgs {
    pub path: String,
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    pub query: String,
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GitDiffArgs {
    #[serde(default)]
    pub staged: bool,
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectArgs {
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SwitchProjectArgs {
    pub project_name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TaskStartArgs {
    pub goal: String,
    pub plan: PlanArgs,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub executor: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PlanArgs {
    pub actions: Vec<String>,
    #[serde(default, deserialize_with = "string_or_vec")]
    #[schemars(with = "Vec<String>")]
    pub tests: Vec<String>,
    pub success_criteria: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TaskIdArgs {
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
}

pub fn string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StringOrVec;
    impl<'de> serde::de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a string or an array of strings")
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(if v.trim().is_empty() {
                Vec::new()
            } else {
                vec![v.to_string()]
            })
        }
        fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
            self.visit_str(&v)
        }
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut out = Vec::new();
            while let Some(s) = seq.next_element::<String>()? {
                if !s.trim().is_empty() {
                    out.push(s);
                }
            }
            Ok(out)
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }
    }
    deserializer.deserialize_any(StringOrVec)
}