/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is dual-licensed under either the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree or the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree. You may select, at your option, one of the
 * above-listed licenses.
 */

use std::path::PathBuf;
use std::sync::Arc;

#[cfg(target_os = "windows")]
use elp_windows::{AbsPath, AbsPathBuf};
#[cfg(not(target_os = "windows"))]
use paths::{AbsPath, AbsPathBuf};
use paths::{RelPath, RelPathBuf};


use elp_base_db::FileId;
use elp_base_db::IncludeCtx;
use elp_base_db::ProjectId;
use elp_base_db::RootQueryDb;
use elp_base_db::SourceDatabase;
use elp_base_db::path_for_file;
use elp_base_db::salsa;
use elp_base_db::salsa::Database;
use elp_erlang_service::Format;
use elp_erlang_service::IncludeType;
use elp_erlang_service::ParseError;
use elp_erlang_service::ParseResult;

use crate::LineIndexDatabase;
use crate::erlang_service::CompileOption;
use crate::erlang_service::ParseRequest;
use crate::metadata;
use crate::metadata::Metadata;

pub trait AstLoader {
    fn load_ast(
        &self,
        project_id: ProjectId,
        file_id: FileId,
        path: &AbsPath,
        macros: &[eetf::Term],
        parse_transforms: &[eetf::Term],
        elp_metadata: eetf::Term,
    ) -> ParseResult;
}

impl AstLoader for crate::RootDatabase {
    fn load_ast(
        &self,
        project_id: ProjectId,
        file_id: FileId,
        path: &AbsPath,
        macros: &[eetf::Term],
        parse_transforms: &[eetf::Term],
        elp_metadata: eetf::Term,
    ) -> ParseResult {
        let mut macros = macros.to_vec();
        macros.push(eetf::Atom::from("ELP_ERLANG_SERVICE").into());
        let options = vec![
            CompileOption::Macros(macros),
            CompileOption::ParseTransforms(parse_transforms.to_vec()),
            CompileOption::ElpMetadata(elp_metadata),
        ];
        let path: PathBuf = path.to_path_buf().into();
        let file_text = SourceDatabase::file_text(self, file_id).text(self);
        let req = ParseRequest {
            options,
            file_id,
            path: path.clone(),
            format: Format::OffsetEtf,
            file_text,
        };
        let erlang_service = self.erlang_service_for(project_id);

        erlang_service.request_parse(
            req,
            || self.unwind_if_revision_cancelled(),
            &move |file_id, include_type, path| resolve_include(self, file_id, include_type, path),
        )
    }
}

fn resolve_include(
    db: &dyn RootQueryDb,
    file_id: FileId,
    include_type: IncludeType,
    path: &str,
) -> Option<(String, FileId, Arc<str>)> {
    let include_file_id = match include_type {
        IncludeType::Normal => IncludeCtx::new(db, file_id).resolve_include(path)?,
        IncludeType::Lib => IncludeCtx::new(db, file_id).resolve_include_lib(path)?,
        IncludeType::Doc => IncludeCtx::new(db, file_id).resolve_include_doc(path)?,
    };
    let path = path_for_file(db, include_file_id).map(|vfs_path| vfs_path.to_string())?;
    Some((
        path,
        include_file_id,
        db.file_text(include_file_id).text(db),
    ))
}

#[ra_ap_query_group_macro::query_group(ErlAstDatabaseStorage)]
pub trait ErlAstDatabase: RootQueryDb + AstLoader + LineIndexDatabase {
    fn module_ast(&self, file_id: FileId) -> Arc<ParseResult>;
    fn elp_metadata(&self, file_id: FileId) -> Metadata;
}

fn module_ast(db: &dyn ErlAstDatabase, file_id: FileId) -> Arc<ParseResult> {
    // Context for T171541590
    let _ = stdx::panic_context::enter(format!("\nmodule_ast: {file_id:?}"));
    let root_id = db.file_source_root(file_id).source_root_id(db);
    let root = db.source_root(root_id).source_root(db);
    let path = root.path_for_file(&file_id).unwrap().as_path().unwrap();
    let metadata = db.elp_metadata(file_id);
    let app_data = if let Some(app_data) = db.file_app_data(file_id) {
        app_data
    } else {
        return Arc::new(ParseResult::error(ParseError {
            path: path.to_path_buf().into(),
            location: None,
            msg: "Unknown application".to_string(),
            code: "L0003".to_string(),
        }));
    };
    Arc::new(db.load_ast(
        app_data.project_id,
        file_id,
        AbsPath::assert_inner(&path),
        &app_data.macros,
        &app_data.parse_transforms,
        metadata.into(),
    ))
}

fn elp_metadata(db: &dyn ErlAstDatabase, file_id: FileId) -> Metadata {
    let line_index = db.file_line_index(file_id);
    let file_text = db.file_text(file_id).text(db);
    let source = db.parse(file_id);
    metadata::collect_metadata(&line_index, &file_text, &source)
}
