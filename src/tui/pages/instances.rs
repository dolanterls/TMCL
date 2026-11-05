use std::path::PathBuf;

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceSummary {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub status: String,
    pub path: PathBuf,
    pub launch_count: u64,
    pub play_time_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceAction {
    Launch,
    Create,
    Import,
    Rename,
    Remove,
}

#[derive(Debug, Clone, Default)]
pub struct InstancePage {
    pub items: Vec<InstanceSummary>,
    pub focused: usize,
    pub current_id: Option<Uuid>,
}

impl InstancePage {
    pub fn selected_item(&self) -> Option<&InstanceSummary> {
        self.focused_item()
    }

    pub fn focused_item(&self) -> Option<&InstanceSummary> {
        self.items.get(self.focused)
    }

    pub fn focus_next(&mut self) {
        if !self.items.is_empty() {
            self.focused = (self.focused + 1).min(self.items.len() - 1);
        }
    }

    pub fn focus_previous(&mut self) {
        self.focused = self.focused.saturating_sub(1);
    }

    pub fn apply(&mut self, items: Vec<InstanceSummary>) {
        self.items = items;
        self.focused = self.focused.min(self.items.len().saturating_sub(1));
        if self
            .current_id
            .is_some_and(|id| !self.items.iter().any(|item| item.id == id))
        {
            self.current_id = None;
        }
    }

    pub fn set_current(&mut self, id: Option<Uuid>) {
        self.current_id = id.filter(|id| self.items.iter().any(|item| item.id == *id));
    }
}

#[cfg(test)]
mod tests {
    use super::{InstancePage, InstanceSummary};
    use uuid::Uuid;

    #[test]
    fn selection_stays_inside_instance_list() {
        let mut page = InstancePage::default();
        page.apply(vec![InstanceSummary {
            id: uuid::Uuid::new_v4(),
            name: "test".into(),
            version: "1.21.1".into(),
            loader: "Vanilla".into(),
            loader_version: None,
            status: "ready".into(),
            path: std::path::PathBuf::from("C:/Games/versions/test"),
            launch_count: 0,
            play_time_seconds: 0,
        }]);
        page.focus_next();
        assert_eq!(page.focused, 0);
        assert_eq!(page.selected_item().expect("item").name, "test");
        page.focus_previous();
        assert_eq!(page.focused, 0);
    }

    #[test]
    fn focus_does_not_change_current_instance() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut page = InstancePage::default();
        page.apply(vec![
            InstanceSummary {
                id: first,
                name: "first".into(),
                version: "1.21.1".into(),
                loader: "Vanilla".into(),
                loader_version: None,
                status: "ready".into(),
                path: std::path::PathBuf::from("first"),
                launch_count: 0,
                play_time_seconds: 0,
            },
            InstanceSummary {
                id: second,
                name: "second".into(),
                version: "1.20.1".into(),
                loader: "Fabric".into(),
                loader_version: Some("0.16.5".into()),
                status: "ready".into(),
                path: std::path::PathBuf::from("second"),
                launch_count: 2,
                play_time_seconds: 90,
            },
        ]);
        page.set_current(Some(first));
        page.focus_next();
        assert_eq!(page.focused_item().expect("focused").id, second);
        assert_eq!(page.current_id, Some(first));
    }
}
