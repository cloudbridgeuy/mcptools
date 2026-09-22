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

#[derive(Debug)]
pub struct Output {
    pub logs: Vec<String>,
    pub result: serde_json::Value,
}

impl Output {
    pub fn check_size(&self, cap: usize) -> Result<(), OutputLimit> {
        let log_bytes: usize = self.logs.iter().map(String::len).sum();
        if log_bytes + self.result.to_string().len() > cap {
            Err(OutputLimit(cap))
        } else {
            Ok(())
        }
    }
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
        let at_cap = Output {
            logs: vec!["xx".to_string()],
            result: serde_json::json!(""),
        };
        assert_eq!(at_cap.check_size(4), Ok(()));
        let over_cap = Output {
            logs: vec!["xxx".to_string()],
            result: serde_json::json!(""),
        };
        assert_eq!(over_cap.check_size(4), Err(OutputLimit(4)));
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
