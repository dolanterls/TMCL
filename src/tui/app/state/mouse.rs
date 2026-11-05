//! Mouse routing: hit map lookup and click classification.
//!
//! A click or a wheel notch resolves against the regions the last frame
//! registered; clicks become an `Action` applied by `apply.rs`, notches pick a
//! `ScrollTarget`. Dialogs route through the same map (stage P4), so a field
//! row, a dropdown option and a key chip are ordinary hit boxes.

use super::*;
use crate::tui::action::{Click, PageAct};
use crate::tui::hit::Layer;

impl App {
    pub(in crate::tui::app) fn handle_mouse(&mut self, mouse: MouseEvent) {
        // The hit map belongs to the frame that drew it. A resize between the
        // draw and the event makes those rectangles wrong, so ignore input
        // until the next frame repaints at the new size.
        if self.hitmap.generation() != self.resize_gen {
            return;
        }

        let top = self.top_layer();
        match mouse.kind {
            MouseEventKind::Moved => self.set_hover(mouse.column, mouse.row, top),
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = self.hitmap.at(mouse.column, mouse.row, top);
                // A click anywhere but the field itself ends the edit. Without
                // this the inline search could only be left with `Esc`, which
                // is a cancel — the mouse had no way to say "done".
                if self.search.is_some()
                    && !matches!(
                        hit.map(|hit| hit.action),
                        Some(Action::Page {
                            act: PageAct::Search(_),
                            ..
                        })
                    )
                {
                    self.blur_search();
                }
                if let Some(hit) = hit {
                    let click = if self.is_double_click(hit.action) {
                        Click::Double
                    } else {
                        Click::Single
                    };
                    self.apply_action(hit.action, click);
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                // Only the region under the pointer scrolls; with no region the
                // notch is ignored instead of falling back to the list below.
                if let Some(target) = self.hitmap.scroll_at(mouse.column, mouse.row, top) {
                    let up = matches!(mouse.kind, MouseEventKind::ScrollUp);
                    self.apply_scroll(target, up);
                }
            }
            _ => {}
        }
    }

    /// Hover is a pure `Copy` value: registering a region is enough to make it
    /// hoverable, so no per-widget wiring is needed.
    fn set_hover(&mut self, column: u16, row: u16, top: Layer) {
        let next = self.hitmap.at(column, row, top).map(|hit| hit.action);
        if next != self.hover {
            self.hover = next;
        }
    }
}
