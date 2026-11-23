//! Reordering: dragging a card renumbers the folders (and the files inside) to match.

use std::collections::{HashMap, HashSet};
use super::App;
use crate::model::Meta;
use crate::ops::renumber_folders;

impl App {
    // ── Reordering ──

    /// Reordering renames folders, so it is only allowed when the list is complete and numeric.
    pub fn reorder_allowed(&self) -> bool {
        self.search.is_empty()
            && self.filter.is_none()
            && self.items.iter().all(|i| i.id.parse::<u64>().is_ok())
    }

    /// Moves the card at index `from` to index `to`, then renumbers folders (and the files
    /// inside) so the number always matches the position: 1, 2, 3…
    pub fn reorder(&mut self, from: usize, to: usize) {
        let Some(root) = self.root.clone() else { return };
        let n = self.items.len();
        if from >= n || to >= n {
            return;
        }
        // New order as a list of old indices.
        let mut order: Vec<usize> = (0..n).collect();
        let moved = order.remove(from);
        order.insert(to, moved);

        // Which folders change number? (old id, new id)
        let mut plan: Vec<(String, String)> = vec![];
        for (k, &idx) in order.iter().enumerate() {
            let old = self.items[idx].id.clone();
            let new = (k + 1).to_string();
            if old != new {
                plan.push((old, new));
            }
        }
        if plan.is_empty() {
            return;
        }
        let sel_old = self.selected.and_then(|i| self.items.get(i)).map(|i| i.id.clone());

        match renumber_folders(&root, &plan) {
            Err(e) => self.msg = e, // nothing was changed
            Ok(problems) => {
                // Status + notes follow their UGC to the new number.
                let new_ids: HashSet<String> = plan.iter().map(|(_, new)| new.clone()).collect();
                let mut new_meta: HashMap<String, Meta> = HashMap::new();
                for (old, new) in &plan {
                    if let Some(m) = self.meta.remove(old) {
                        new_meta.insert(new.clone(), m);
                    }
                }
                for (k, v) in self.meta.drain() {
                    if !new_ids.contains(&k) {
                        new_meta.insert(k, v); // untouched UGCs keep theirs
                    }
                }
                self.meta = new_meta;
                self.save_meta();

                // Keep the same UGC selected (it now has a new number).
                let sel_new = sel_old.map(|o| {
                    plan.iter().find(|(a, _)| *a == o).map(|(_, b)| b.clone()).unwrap_or(o)
                });
                self.reload(sel_new);
                self.msg = if problems.is_empty() {
                    "Reordered: folders and files were renamed to match the new order.".into()
                } else {
                    format!("Reordered, but: {}", problems.join("; "))
                };
            }
        }
    }
}
