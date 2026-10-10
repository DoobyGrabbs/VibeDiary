//! The recovery-key window (Settings → Recovery key) and the gentle reminder to set one up.

use super::*;

const NOTE: &str = "A recovery key is a long code that can open your diary if you forget your password. \
Keep it somewhere safe, away from this PC if you can (written down, or in a password manager). \
Anyone who has both the key and your diary file can open your diary.";

impl DiaryApp {
    /// The text saved by "Save as text file…".
    fn recovery_file_text(key: &str) -> String {
        format!(
            "VibeDiary recovery key\r\n\r\nRecovery key: {key}\r\nMade on: {}\r\n\r\n{NOTE}\r\n\r\n\
             To use it: on the unlock screen choose \"Forgot your password?\", type the key, and pick a new password.\r\n",
            Local::now().format("%Y-%m-%d")
        )
    }

    pub(crate) fn recovery_window(&mut self, ctx: &egui::Context) {
        if !self.show_recovery {
            return;
        }
        let Some(vault) = self.vault.as_ref() else { return };
        let (has, created) = (vault.has_recovery(), vault.recovery_created().map(str::to_string));
        let p = palette(ctx);
        let dark = ctx.theme() == egui::Theme::Dark;
        let showing = self.recovery_shown.clone();
        let mut close = false;
        let (mut create, mut remove, mut done, mut copy, mut save_file) = (false, false, false, false, false);
        let arm = self.recovery_arm;
        let mut confirmed = self.recovery_confirmed;
        let mut new_arm = arm;

        egui::Window::new("Recovery key")
            .title_bar(false)
            .frame(popup_frame(&p, dark))
            .resizable(false)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                // While a new key is on screen the window can only be closed with "Done".
                if popup_header(ui, &p, "Recovery key") && showing.is_none() {
                    close = true;
                }
                egui::Frame::new().inner_margin(14).show(ui, |ui| {
                    ui.set_width(480.0);
                    if let Some(key) = &showing {
                        ui.label(RichText::new("Your recovery key").size(20.0).strong().color(p.title));
                        ui.add_space(4.0);
                        ui.label(RichText::new(NOTE).color(p.muted));
                        ui.add_space(8.0);
                        egui::Frame::new().fill(p.head_bg).corner_radius(8).inner_margin(14).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add(egui::Label::new(RichText::new(key).size(22.0).strong().monospace().color(p.head_text)).selectable(true));
                        });
                        ui.add_space(6.0);
                        ui.label(RichText::new("This is the only time it is shown. If you lose it you can make a new one here.").strong().color(p.ink));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            copy = ui.button("Copy").clicked();
                            save_file = ui.button("Save as text file…").clicked();
                        });
                        ui.add_space(8.0);
                        ui.checkbox(&mut confirmed, "I have stored my recovery key somewhere safe");
                        ui.add_space(6.0);
                        done = ui.add_enabled(confirmed, egui::Button::new("Done")).clicked();
                    } else if has {
                        ui.label(RichText::new("A recovery key is set up").size(20.0).strong().color(p.title));
                        if let Some(d) = &created {
                            ui.label(RichText::new(format!("Made on {d}")).color(p.muted));
                        }
                        ui.add_space(6.0);
                        ui.label(RichText::new("If you forget your password, choose \"Forgot your password?\" on the unlock screen and enter your recovery key.").color(p.ink));
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            let replace = if arm == 1 { "Click again: the old key stops working" } else { "Make a new recovery key" };
                            if ui.button(replace).clicked() {
                                if arm == 1 {
                                    create = true;
                                    new_arm = 0;
                                } else {
                                    new_arm = 1;
                                }
                            }
                        });
                        ui.add_space(4.0);
                        let del = if arm == 2 { "Click again to remove it" } else { "Remove the recovery key" };
                        if ui.add(egui::Button::new(RichText::new(del).color(Color32::WHITE)).fill(RED)).clicked() {
                            if arm == 2 {
                                remove = true;
                                new_arm = 0;
                            } else {
                                new_arm = 2;
                            }
                        }
                        ui.add_space(6.0);
                        ui.label(RichText::new("Without a recovery key a forgotten password can't be recovered by anyone.").small().color(p.muted));
                    } else {
                        ui.label(RichText::new("No recovery key yet").size(20.0).strong().color(p.title));
                        ui.add_space(4.0);
                        ui.label(RichText::new(NOTE).color(p.muted));
                        ui.add_space(6.0);
                        ui.label(RichText::new("Without one, a forgotten password can't be recovered by anyone.").strong().color(p.ink));
                        ui.add_space(10.0);
                        create = ui.button("Create a recovery key").clicked();
                    }
                    if !self.recovery_msg.is_empty() {
                        ui.add_space(8.0);
                        ui.add(egui::Label::new(RichText::new(&self.recovery_msg).color(Color32::from_rgb(239, 68, 68))).wrap());
                    }
                });
            });

        self.recovery_confirmed = confirmed;
        self.recovery_arm = new_arm;
        if create {
            self.recovery_msg.clear();
            match self.vault.as_mut().map(|v| v.set_recovery()) {
                Some(Ok(key)) => {
                    self.recovery_shown = Some(key);
                    self.recovery_confirmed = false;
                    self.persist();
                    if self.status.starts_with("Save failed") {
                        self.recovery_msg = format!("The recovery key could not be saved with the diary. {}", self.status);
                    }
                }
                Some(Err(e)) => self.recovery_msg = e,
                None => {}
            }
        }
        if remove {
            if let Some(v) = self.vault.as_mut() {
                v.remove_recovery();
            }
            self.persist();
        }
        if copy {
            if let Some(key) = &self.recovery_shown {
                ctx.copy_text(key.clone());
                self.recovery_msg.clear();
            }
        }
        if save_file {
            if let Some(key) = self.recovery_shown.clone() {
                if let Some(dest) = rfd::FileDialog::new().set_file_name("diary-recovery-key.txt").save_file() {
                    self.recovery_msg = match std::fs::write(&dest, Self::recovery_file_text(&key)) {
                        Ok(()) => String::new(),
                        Err(e) => format!("Couldn't save the file: {e}"),
                    };
                }
            }
        }
        if done {
            self.recovery_shown = None;
            self.recovery_confirmed = false;
            self.show_recovery = false;
            self.recovery_msg.clear();
        }
        if close {
            self.show_recovery = false;
            self.recovery_arm = 0;
            self.recovery_msg.clear();
        }
    }

    /// A one-line reminder in the day panel while the diary has no recovery key.
    pub(crate) fn recovery_hint(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let missing = self.vault.as_ref().is_some_and(|v| !v.has_recovery());
        if !missing || self.recovery_hint_dismissed || self.data.entries.len() < 3 {
            return;
        }
        ui.add_space(6.0);
        egui::Frame::new().fill(p.head_bg).corner_radius(8).inner_margin(8).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("No recovery key yet. If you forget your password, nobody can open the diary.").small().color(p.head_text));
            ui.horizontal(|ui| {
                if ui.small_button("Set one up").clicked() {
                    self.show_recovery = true;
                }
                if ui.small_button("Not now").clicked() {
                    self.recovery_hint_dismissed = true;
                }
            });
        });
    }
}
