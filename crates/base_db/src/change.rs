/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is dual-licensed under either the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree or the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree. You may select, at your option, one of the
 * above-listed licenses.
 */

//! Defines a unit of change that can applied to the database to get the next
//! state. Changes are transactional.

use std::fmt;
use std::sync::Arc;

#[cfg(target_os = "windows")]
use elp_windows::{AbsPath, AbsPathBuf};
#[cfg(not(target_os = "windows"))]
use paths::{AbsPath, AbsPathBuf};
use paths::{RelPath, RelPathBuf};
use vfs::FileId;

use crate::RootQueryDb;
use crate::SourceRoot;
use crate::SourceRootId;
use crate::input::AppStructure;

/// Encapsulate a bunch of raw `.set` calls on the database.
#[derive(Clone, Default)]
pub struct Change {
    pub roots: Option<Vec<SourceRoot>>,
    pub files_changed: Vec<(FileId, Option<Arc<str>>)>,
    pub app_structure: Option<AppStructure>,
}

impl fmt::Debug for Change {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut d = fmt.debug_struct("Change");
        if let Some(roots) = &self.roots {
            d.field("roots", roots);
        }
        if !self.files_changed.is_empty() {
            d.field("files_changed", &self.files_changed.len());
        }
        if self.app_structure.is_some() {
            d.field("app_structure", &self.app_structure);
        }
        d.finish()
    }
}

impl Change {
    pub fn new() -> Change {
        Change::default()
    }

    pub fn set_roots(&mut self, roots: Vec<SourceRoot>) {
        self.roots = Some(roots);
    }

    pub fn change_file(&mut self, file_id: FileId, new_text: Option<Arc<str>>) {
        self.files_changed.push((file_id, new_text))
    }

    pub fn set_app_structure(&mut self, a: AppStructure) {
        self.app_structure = Some(a);
    }

    pub fn apply(
        self,
        db: &mut dyn RootQueryDb,
        resolve_file_id: &impl Fn(&AbsPathBuf) -> Option<FileId>,
    ) -> Vec<FileId> {
        let _p = tracing::info_span!("RootDatabase::apply_change").entered();
        if let Some(roots) = self.roots {
            for (idx, root) in roots.into_iter().enumerate() {
                let root_id = SourceRootId(idx as u32);
                for file_id in root.iter() {
                    db.set_file_source_root(file_id, root_id);
                }
                db.set_source_root(root_id, Arc::new(root));
            }
        }

        if let Some(set_app_structure) = self.app_structure {
            set_app_structure.apply(db, resolve_file_id);
        }

        let mut res = vec![];
        for (file_id, text) in self.files_changed {
            // XXX: can't actually remove the file, just reset the text

            let text = text.unwrap_or_else(|| Arc::from(""));
            db.set_file_text(file_id, text);
            res.push(file_id);
        }
        res
    }
}
