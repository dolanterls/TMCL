//! Settings-owned forms that are not single-value editors: game directories
//! and Java runtimes still need their own small dialogs.

use super::*;

impl App {
    pub(in crate::tui::app) fn open_game_directory_form(&mut self) {
        self.modal = Some(
            Modal::form(
                self.i18n.text("form.add_game_directory"),
                &[self.i18n.text("form.directory_path")],
                &[String::new()],
                FormAction::AddGameDirectory,
            )
            .with_directory_picker(0),
        );
    }

    pub(in crate::tui::app) fn open_java_runtime_form(&mut self) {
        let major = self
            .java
            .get(self.java_selected)
            .map(|java| java.major)
            .unwrap_or(21);
        self.modal = Some(Modal::form(
            self.i18n.text("form.install_java"),
            &[self.i18n.text("form.java_major")],
            &[major.to_string()],
            FormAction::DownloadJavaRuntime,
        ));
    }

    pub(in crate::tui::app) fn open_java_add_form(&mut self) {
        self.modal = Some(
            Modal::form(
                self.i18n.text("form.add_java"),
                &[self.i18n.text("form.java_executable")],
                &[String::new()],
                FormAction::AddJavaRuntime,
            )
            .with_directory_picker(0),
        );
    }
}
