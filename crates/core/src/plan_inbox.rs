use std::path::Path;

pub fn canvas_message(_id: &str, _rev: u64, _diff: &str, _png: &Path) -> Result<String, String> {
    Err("GUZ-1143: canvas message builder not implemented".into())
}

#[cfg(test)]
mod tests {
    use super::canvas_message;
    use std::path::Path;

    #[test]
    #[ignore = "GUZ-1143"]
    fn canvas_input_names_plan_revision_diff_and_absolute_image() {
        let diff = "+ text note \"phone edit\"\n~ start position: (0, 0) -> (42, -17)";
        let message = canvas_message("my-plan", 42, diff, Path::new("/plans/my plan/latest.png"))
            .expect("canvas message builder must accept saved plan input");
        assert!(message.to_lowercase().contains("canvas input"));
        assert!(message.contains("my-plan"));
        assert!(message.lines().next().unwrap().contains("42"));
        assert!(message.contains(diff));
        assert!(message.contains("[Image #1: /plans/my plan/latest.png]"));
    }

    #[test]
    #[ignore = "GUZ-1143"]
    fn unchanged_canvas_still_has_input_and_image() {
        let message = canvas_message("unchanged", 2, "", Path::new("/plans/unchanged/latest.png"))
            .expect("an empty diff must not suppress a successful save message");
        assert!(message.to_lowercase().contains("canvas input"));
        assert!(message.contains("unchanged"));
        assert!(message.contains('2'));
        assert!(message.contains("[Image #1: /plans/unchanged/latest.png]"));
    }
}
