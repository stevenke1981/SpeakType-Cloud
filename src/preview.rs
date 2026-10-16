/// Projection state is separate from the user's unsent edit draft.
#[derive(Default)]
pub struct PreviewProjection {
    last_revision: Option<u64>,
}
impl PreviewProjection {
    pub fn project(&mut self, revision: u64, text: &str) -> Option<String> {
        if self.last_revision == Some(revision) {
            return None;
        }
        self.last_revision = Some(revision);
        Some(text.into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equal_completions_reset_unsent_edit_but_timer_ticks_preserve_it() {
        let mut projection = PreviewProjection::default();
        let mut draft = projection.project(1, "A").expect("first result");
        assert_eq!(draft, "A");
        draft = "B".into();
        if let Some(value) = projection.project(1, "A") {
            draft = value;
        }
        assert_eq!(draft, "B");
        if let Some(value) = projection.project(2, "A") {
            draft = value;
        }
        assert_eq!(draft, "A");
    }
}
