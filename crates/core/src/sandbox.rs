use std::time::Duration;

pub struct Limits {
    pub timeout: Duration,
    pub memory_bytes: usize,
    pub output_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            memory_bytes: 64 * 1024 * 1024,
            output_bytes: 256 * 1024,
        }
    }
}

#[derive(Debug)]
pub struct LogBuffer {
    lines: Vec<String>,
    bytes: usize,
    cap: usize,
}

impl LogBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            lines: Vec::new(),
            bytes: 0,
            cap,
        }
    }

    pub fn push(&mut self, line: String) -> Result<(), OutputLimit> {
        if self.bytes + line.len() > self.cap {
            return Err(OutputLimit(self.cap));
        }
        self.bytes += line.len();
        self.lines.push(line);
        Ok(())
    }

    pub fn into_lines(self) -> Vec<String> {
        self.lines
    }
}

#[derive(Debug, PartialEq)]
pub struct OutputLimit(pub usize);

pub struct Run {
    pub logs: Vec<String>,
    pub outcome: Result<serde_json::Value, SandboxError>,
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ExecuteOutput {
    pub logs: Vec<String>,
    pub result: serde_json::Value,
    pub error: Option<ExecuteError>,
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ExecuteError {
    pub message: String,
    pub name: String,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum SandboxError {
    #[error("execution timed out after {0:?}")]
    Timeout(Duration),
    #[error("output limit exceeded: {0} bytes")]
    OutputLimit(usize),
    #[error("{name}: {message}")]
    Js { name: String, message: String },
}

impl From<OutputLimit> for SandboxError {
    fn from(OutputLimit(cap): OutputLimit) -> Self {
        Self::OutputLimit(cap)
    }
}

pub fn check_size(
    logs: &[String],
    result: &serde_json::Value,
    cap: usize,
) -> Result<(), OutputLimit> {
    let log_bytes: usize = logs.iter().map(String::len).sum();
    if log_bytes + result.to_string().len() > cap {
        Err(OutputLimit(cap))
    } else {
        Ok(())
    }
}

pub fn execute_output(run: Run) -> ExecuteOutput {
    let Run { logs, outcome } = run;
    match outcome {
        Ok(result) => ExecuteOutput {
            logs,
            result,
            error: None,
        },
        Err(err) => {
            let error = match &err {
                SandboxError::Timeout(_) => ExecuteError {
                    name: "TimeoutError".to_string(),
                    message: err.to_string(),
                },
                SandboxError::OutputLimit(_) => ExecuteError {
                    name: "OutputLimitError".to_string(),
                    message: err.to_string(),
                },
                SandboxError::Js { name, message } => ExecuteError {
                    name: name.clone(),
                    message: message.clone(),
                },
            };
            ExecuteOutput {
                logs,
                result: serde_json::Value::Null,
                error: Some(error),
            }
        }
    }
}

pub fn tool_result(envelope: serde_json::Value) -> serde_json::Value {
    envelope
        .get("structuredContent")
        .cloned()
        .unwrap_or(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_at_cap_is_accepted() {
        let mut buffer = LogBuffer::new(4);
        assert!(buffer.push("abcd".to_string()).is_ok());
    }

    #[test]
    fn push_over_cap_is_rejected() {
        let mut buffer = LogBuffer::new(4);
        assert_eq!(buffer.push("abcde".to_string()), Err(OutputLimit(4)));
    }

    #[test]
    fn check_size_boundary() {
        let at_logs = vec!["xx".to_string()];
        assert_eq!(check_size(&at_logs, &serde_json::json!(""), 4), Ok(()));
        let over_logs = vec!["xxx".to_string()];
        assert_eq!(
            check_size(&over_logs, &serde_json::json!(""), 4),
            Err(OutputLimit(4))
        );
    }

    #[test]
    fn execute_output_ok_passes_logs_through() {
        let output = execute_output(Run {
            logs: vec!["a".to_string()],
            outcome: Ok(serde_json::json!(2)),
        });
        assert_eq!(
            output,
            ExecuteOutput {
                logs: vec!["a".to_string()],
                result: serde_json::json!(2),
                error: None,
            }
        );
    }

    #[test]
    fn execute_output_timeout_names_timeout_error() {
        let err = SandboxError::Timeout(Duration::from_secs(1));
        let message = err.to_string();
        let output = execute_output(Run {
            logs: vec!["a".to_string()],
            outcome: Err(err),
        });
        assert_eq!(
            output,
            ExecuteOutput {
                logs: vec!["a".to_string()],
                result: serde_json::Value::Null,
                error: Some(ExecuteError {
                    name: "TimeoutError".to_string(),
                    message,
                }),
            }
        );
    }

    #[test]
    fn execute_output_limit_names_output_limit_error() {
        let err = SandboxError::OutputLimit(4);
        let message = err.to_string();
        let output = execute_output(Run {
            logs: vec!["a".to_string()],
            outcome: Err(err),
        });
        assert_eq!(
            output,
            ExecuteOutput {
                logs: vec!["a".to_string()],
                result: serde_json::Value::Null,
                error: Some(ExecuteError {
                    name: "OutputLimitError".to_string(),
                    message,
                }),
            }
        );
    }

    #[test]
    fn execute_output_js_passes_name_and_message_through() {
        let output = execute_output(Run {
            logs: vec!["a".to_string()],
            outcome: Err(SandboxError::Js {
                name: "TypeError".to_string(),
                message: "x".to_string(),
            }),
        });
        assert_eq!(
            output,
            ExecuteOutput {
                logs: vec!["a".to_string()],
                result: serde_json::Value::Null,
                error: Some(ExecuteError {
                    name: "TypeError".to_string(),
                    message: "x".to_string(),
                }),
            }
        );
    }

    #[test]
    fn tool_result_prefers_structured_content() {
        let envelope = serde_json::json!({
            "content": [{"type": "text", "text": "{}"}],
            "structuredContent": {"tools": []},
        });
        assert_eq!(tool_result(envelope), serde_json::json!({"tools": []}));
    }

    #[test]
    fn tool_result_falls_back_to_envelope() {
        let envelope = serde_json::json!({
            "content": [{"type": "text", "text": "{}"}],
        });
        assert_eq!(tool_result(envelope.clone()), envelope);
    }
}
