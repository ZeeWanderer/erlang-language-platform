/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is dual-licensed under either the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree or the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree. You may select, at your option, one of the
 * above-listed licenses.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Instant;

#[cfg(target_os = "windows")]
use elp_windows::{AbsPath, AbsPathBuf};
#[cfg(not(target_os = "windows"))]
use paths::{AbsPath, AbsPathBuf};
use paths::{RelPath, RelPathBuf};

use elp_base_db::AppType;
use elp_base_db::FileId;
use elp_base_db::ModuleName;
use elp_base_db::ProjectId;
use elp_base_db::RootQueryDb;
use elp_types_db::StringId;
use elp_types_db::eqwalizer::AST;
use elp_types_db::eqwalizer::Id;
use elp_types_db::eqwalizer::form::Callback;
use elp_types_db::eqwalizer::form::ExternalForm;
use elp_types_db::eqwalizer::form::FunSpec;
use elp_types_db::eqwalizer::form::OverloadedFunSpec;
use elp_types_db::eqwalizer::form::RecDecl;
use elp_types_db::eqwalizer::form::TypeDecl;
use parking_lot::Mutex;

use crate::EqwalizerConfig;
use crate::EqwalizerDiagnostics;
use crate::ast;
use crate::ast::Error;
use crate::ast::Visibility;
use crate::ast::contractivity::StubContractivityChecker;
use crate::ast::expand::StubExpander;
use crate::ast::stub::ModuleStub;
use crate::ast::stub::VStub;
use crate::ast::trans_valid::TransitiveChecker;
use crate::get_module_diagnostics;
use crate::ipc::IpcHandle;

pub trait EqwalizerErlASTStorage {
    fn erl_ast_bytes(
        &self,
        project_id: ProjectId,
        module: ModuleName,
    ) -> Result<Arc<Vec<u8>>, Error>;

    fn eqwalizer_ast(&self, project_id: ProjectId, module: ModuleName) -> Result<Arc<AST>, Error> {
        let ast = self.erl_ast_bytes(project_id, module)?;
        ast::from_bytes(&ast, false).map(Arc::new)
    }

    fn eqwalizer_ast_bytes(
        &self,
        project_id: ProjectId,
        module: ModuleName,
    ) -> Result<Arc<Vec<u8>>, Error> {
        self.eqwalizer_ast(project_id, module).map(|ast| {
            Arc::new(ast::to_bytes(
                &ast.forms.iter().filter(is_non_stub_form).collect(),
            ))
        })
    }
}

pub trait ELPDbApi {
    fn eqwalizing_start(&self, module: String);
    fn eqwalizing_done(&self, module: String);
    fn set_module_ipc_handle(&self, module: ModuleName, handle: Option<Arc<Mutex<IpcHandle>>>);
    fn module_ipc_handle(&self, module: ModuleName) -> Option<Arc<Mutex<IpcHandle>>>;
}

#[ra_ap_query_group_macro::query_group]
pub trait EqwalizerDiagnosticsDatabase: EqwalizerErlASTStorage + RootQueryDb + ELPDbApi {
    #[salsa::input]
    fn eqwalizer_config(&self) -> Arc<EqwalizerConfig>;

    fn module_diagnostics(
        &self,
        project_id: ProjectId,
        module: String,
    ) -> (Arc<EqwalizerDiagnostics>, Instant);

    fn converted_stub(&self, project_id: ProjectId, module: ModuleName) -> Result<Arc<AST>, Error>;

    fn type_ids(
        &self,
        project_id: ProjectId,
        module: ModuleName,
    ) -> Result<Arc<BTreeMap<Id, Visibility>>, Error>;

    fn expanded_stub(
        &self,
        project_id: ProjectId,
        module: ModuleName,
    ) -> Result<Arc<ModuleStub>, Error>;

    fn contractive_stub(
        &self,
        project_id: ProjectId,
        module: ModuleName,
    ) -> Result<Arc<VStub>, Error>;

    fn transitive_stub(
        &self,
        project_id: ProjectId,
        module: ModuleName,
    ) -> Result<Arc<ModuleStub>, Error>;

    fn transitive_stub_bytes(
        &self,
        project_id: ProjectId,
        module: ModuleName,
    ) -> Result<Arc<Vec<u8>>, Error>;

    fn custom_types(
        &self,
        project_id: ProjectId,
    ) -> Result<Arc<BTreeMap<ModuleName, BTreeMap<Id, Arc<TypeDecl>>>>, Error>;

    fn type_decl(
        &self,
        project_id: ProjectId,
        module: ModuleName,
        id: Id,
    ) -> Result<Option<Arc<TypeDecl>>, Error>;

    fn type_decl_bytes(
        &self,
        project_id: ProjectId,
        module: ModuleName,
        id: Id,
    ) -> Result<Option<Arc<Vec<u8>>>, Error>;

    fn rec_decl(
        &self,
        project_id: ProjectId,
        module: ModuleName,
        id: StringId,
    ) -> Result<Option<Arc<RecDecl>>, Error>;

    fn rec_decl_bytes(
        &self,
        project_id: ProjectId,
        module: ModuleName,
        id: StringId,
    ) -> Result<Option<Arc<Vec<u8>>>, Error>;

    fn fun_spec(
        &self,
        project_id: ProjectId,
        module: ModuleName,
        id: Id,
    ) -> Result<Option<Arc<FunSpec>>, Error>;

    fn fun_spec_bytes(
        &self,
        project_id: ProjectId,
        module: ModuleName,
        id: Id,
    ) -> Result<Option<Arc<Vec<u8>>>, Error>;

    fn overloaded_fun_spec(
        &self,
        project_id: ProjectId,
        module: ModuleName,
        id: Id,
    ) -> Result<Option<Arc<OverloadedFunSpec>>, Error>;

    fn overloaded_fun_spec_bytes(
        &self,
        project_id: ProjectId,
        module: ModuleName,
        id: Id,
    ) -> Result<Option<Arc<Vec<u8>>>, Error>;

    fn custom_fun_specs(
        &self,
        project_id: ProjectId,
    ) -> Result<Arc<BTreeMap<ModuleName, BTreeMap<Id, Arc<FunSpec>>>>, Error>;

    fn custom_overloaded_fun_specs(
        &self,
        project_id: ProjectId,
    ) -> Result<Arc<BTreeMap<ModuleName, BTreeMap<Id, Arc<OverloadedFunSpec>>>>, Error>;

    fn callbacks(
        &self,
        project_id: ProjectId,
        module: ModuleName,
    ) -> Result<(Arc<Vec<Callback>>, Arc<BTreeSet<Id>>), Error>;

    fn callbacks_bytes(
        &self,
        project_id: ProjectId,
        module: ModuleName,
    ) -> Result<Option<Arc<Vec<u8>>>, Error>;
}

fn module_diagnostics(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: String,
) -> (Arc<EqwalizerDiagnostics>, Instant) {
    // A timestamp is added to the return value to force Salsa to store new
    // diagnostics, and not attempt to back-date them if they are equal to
    // the memoized ones.
    let timestamp = Instant::now();
    // Dummy read eqWAlizer config for Salsa
    // Ideally, the config should be passed per module to eqWAlizer instead
    // of being set in the command's environment
    let _ = db.eqwalizer_config();
    match get_module_diagnostics(db, project_id, module.clone()) {
        Ok(diag) => (Arc::new(diag), timestamp),
        Err(err) => (
            Arc::new(EqwalizerDiagnostics::Error(format!(
                "eqWAlizing module {module}:\n{err}"
            ))),
            timestamp,
        ),
    }
}

fn is_non_stub_form(form: &&ExternalForm) -> bool {
    match form {
        ExternalForm::Module(_) => true,
        ExternalForm::FunDecl(_) => true,
        ExternalForm::File(_) => true,
        ExternalForm::ElpMetadata(_) => true,
        ExternalForm::Behaviour(_) => true,
        ExternalForm::EqwalizerNowarnFunction(_) => true,
        ExternalForm::EqwalizerUnlimitedRefinement(_) => true,
        _ => false,
    }
}

fn converted_stub(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
) -> Result<Arc<AST>, Error> {
    if let Some(file_id) = db.module_index(project_id).file_for_module(&module) {
        if let Some(beam_path) = from_beam_path(db, file_id, &module) {
            if let Ok(beam_contents) = std::fs::read(&beam_path) {
                ast::from_beam(&beam_contents).map(Arc::new)
            } else {
                Err(Error::BEAMNotFound(beam_path.into()))
            }
        } else {
            let ast = db.erl_ast_bytes(project_id, module)?;
            ast::from_bytes(&ast, true).map(Arc::new)
        }
    } else {
        Err(Error::ModuleNotFound(module.as_str().into()))
    }
}

fn from_beam_path(
    db: &dyn EqwalizerDiagnosticsDatabase,
    file_id: FileId,
    module: &ModuleName,
) -> Option<AbsPathBuf> {
    let app_data = db.file_app_data(file_id)?;
    if app_data.app_type != AppType::Otp {
        // Only OTP modules are loaded from BEAM
        return None;
    }
    let ebin = app_data.ebin_path.as_ref()?;
    let filename = format!("{}.beam", module.as_str());
    Some(ebin.join(filename))
}

fn type_ids(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
) -> Result<Arc<BTreeMap<Id, Visibility>>, Error> {
    db.converted_stub(project_id, module)
        .map(|ast| Arc::new(ast::type_ids(&ast)))
}

fn expanded_stub(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
) -> Result<Arc<ModuleStub>, Error> {
    let ast = db.converted_stub(project_id, module.clone())?;
    let mut expander = StubExpander::new(db, project_id, module.as_str().into(), &ast);
    expander
        .expand(&ast.forms)
        .map(|()| Arc::new(expander.stub))
        .map_err(Error::TypeConversionError)
}

fn contractive_stub(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
) -> Result<Arc<VStub>, Error> {
    let stub = db.expanded_stub(project_id, module.clone())?;
    let mut checker = StubContractivityChecker::new(db, project_id, module.as_str().into());
    Ok(Arc::new(checker.check(stub)))
}

fn transitive_stub(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
) -> Result<Arc<ModuleStub>, Error> {
    let v_stub = db.contractive_stub(project_id, module.clone())?;
    let mut checker = TransitiveChecker::new(db, project_id, module.as_str().into());
    Ok(Arc::new(checker.check(&v_stub)))
}

fn transitive_stub_bytes(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
) -> Result<Arc<Vec<u8>>, Error> {
    db.transitive_stub(project_id, module)
        .map(|stub| Arc::new(stub.to_bytes()))
}

static EQWALIZER_TYPES: LazyLock<ModuleName> = LazyLock::new(|| ModuleName::new("eqwalizer_types"));

fn custom_types(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
) -> Result<Arc<BTreeMap<ModuleName, BTreeMap<Id, Arc<TypeDecl>>>>, Error> {
    match db.transitive_stub(project_id, EQWALIZER_TYPES.clone()) {
        Ok(stub) => {
            let mut result: BTreeMap<ModuleName, BTreeMap<Id, Arc<TypeDecl>>> = BTreeMap::new();
            for (id, type_decl) in stub.types.iter() {
                let (module_name, ty_name) = id.name.split_once(':').unwrap();
                let module = ModuleName::new(module_name);
                let id = Id {
                    name: StringId::from(ty_name),
                    arity: id.arity,
                };
                result
                    .entry(module)
                    .or_default()
                    .insert(id, type_decl.clone());
            }
            Ok(Arc::new(result))
        }
        // if there is no eqwalizer_types module, return empty map
        Err(Error::ModuleNotFound(_)) => Ok(Arc::new(BTreeMap::new())),
        Err(err) => Err(err),
    }
}

fn type_decl(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
    id: Id,
) -> Result<Option<Arc<TypeDecl>>, Error> {
    let custom_types = db.custom_types(project_id)?;
    // return custom type if it exists
    if let Some(t) = custom_types.get(&module).and_then(|m| m.get(&id)) {
        return Ok(Some(t.clone()));
    }
    let stub = db.transitive_stub(project_id, module)?;
    Ok(stub.types.get(&id).cloned())
}

fn type_decl_bytes(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
    id: Id,
) -> Result<Option<Arc<Vec<u8>>>, Error> {
    db.type_decl(project_id, module, id)
        .map(|t| t.map(|t| Arc::new(t.to_bytes())))
}

fn rec_decl(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
    id: StringId,
) -> Result<Option<Arc<RecDecl>>, Error> {
    let stub = db.transitive_stub(project_id, module)?;
    Ok(stub.records.get(&id).cloned())
}

fn rec_decl_bytes(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
    id: StringId,
) -> Result<Option<Arc<Vec<u8>>>, Error> {
    db.rec_decl(project_id, module, id)
        .map(|t| t.map(|t| Arc::new(t.to_bytes())))
}

fn fun_spec(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
    id: Id,
) -> Result<Option<Arc<FunSpec>>, Error> {
    let custom_overloaded_fun_specs = db.custom_overloaded_fun_specs(project_id)?;
    if custom_overloaded_fun_specs
        .get(&module)
        .and_then(|m| m.get(&id))
        .is_some()
    {
        return Ok(None);
    }
    let custom_fun_specs = db.custom_fun_specs(project_id)?;
    if let Some(fun_spec) = custom_fun_specs.get(&module).and_then(|m| m.get(&id)) {
        return Ok(Some(fun_spec.clone()));
    }
    let stub = db.transitive_stub(project_id, module)?;
    Ok(stub.specs.get(&id).cloned())
}

fn fun_spec_bytes(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
    id: Id,
) -> Result<Option<Arc<Vec<u8>>>, Error> {
    db.fun_spec(project_id, module, id)
        .map(|t| t.map(|t| Arc::new(t.to_bytes())))
}

fn overloaded_fun_spec(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
    id: Id,
) -> Result<Option<Arc<OverloadedFunSpec>>, Error> {
    let custom_fun_specs = db.custom_fun_specs(project_id)?;
    if custom_fun_specs
        .get(&module)
        .and_then(|m| m.get(&id))
        .is_some()
    {
        return Ok(None);
    }
    let custom_overloaded_fun_specs = db.custom_overloaded_fun_specs(project_id)?;
    if let Some(overloaded_fun_spec) = custom_overloaded_fun_specs
        .get(&module)
        .and_then(|m| m.get(&id))
    {
        return Ok(Some(overloaded_fun_spec.clone()));
    }
    let stub = db.transitive_stub(project_id, module)?;
    Ok(stub.overloaded_specs.get(&id).cloned())
}

fn overloaded_fun_spec_bytes(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
    id: Id,
) -> Result<Option<Arc<Vec<u8>>>, Error> {
    db.overloaded_fun_spec(project_id, module, id)
        .map(|t| t.map(|t| Arc::new(t.to_bytes())))
}

static EQWALIZER_SPECS: LazyLock<ModuleName> = LazyLock::new(|| ModuleName::new("eqwalizer_specs"));

fn custom_fun_specs(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
) -> Result<Arc<BTreeMap<ModuleName, BTreeMap<Id, Arc<FunSpec>>>>, Error> {
    match db.transitive_stub(project_id, EQWALIZER_SPECS.clone()) {
        Ok(stub) => {
            let mut result: BTreeMap<ModuleName, BTreeMap<Id, Arc<FunSpec>>> = BTreeMap::new();
            for (id, fun_spec) in stub.specs.iter() {
                let (module_name, ty_name) = id.name.split_once(':').unwrap();
                let module = ModuleName::new(module_name);
                let id = Id {
                    name: StringId::from(ty_name),
                    arity: id.arity,
                };
                result
                    .entry(module)
                    .or_default()
                    .insert(id, fun_spec.clone());
            }
            Ok(Arc::new(result))
        }
        // if there is no eqwalizer_specs module, return an empty map
        Err(Error::ModuleNotFound(_)) => Ok(Arc::new(BTreeMap::new())),
        Err(err) => Err(err),
    }
}

fn custom_overloaded_fun_specs(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
) -> Result<Arc<BTreeMap<ModuleName, BTreeMap<Id, Arc<OverloadedFunSpec>>>>, Error> {
    match db.transitive_stub(project_id, EQWALIZER_SPECS.clone()) {
        Ok(stub) => {
            let mut result: BTreeMap<ModuleName, BTreeMap<Id, Arc<OverloadedFunSpec>>> =
                BTreeMap::new();
            for (id, overloaded_fun_spec) in stub.overloaded_specs.iter() {
                let parts: Vec<&str> = id.name.split(":").collect();
                let module = ModuleName::new(parts[0]);
                let ty_name = parts[1];
                let id = Id {
                    name: ty_name.to_string().into(),
                    arity: id.arity,
                };
                result
                    .entry(module)
                    .or_default()
                    .insert(id, overloaded_fun_spec.clone());
            }
            Ok(Arc::new(result))
        }
        // if there is no eqwalizer_specs module, return empty map
        Err(Error::ModuleNotFound(_)) => Ok(Arc::new(BTreeMap::new())),
        Err(err) => Err(err),
    }
}

fn callbacks(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
) -> Result<(Arc<Vec<Callback>>, Arc<BTreeSet<Id>>), Error> {
    let stub = db.transitive_stub(project_id, module)?;
    Ok((stub.callbacks.clone(), stub.optional_callbacks.clone()))
}

fn callbacks_bytes(
    db: &dyn EqwalizerDiagnosticsDatabase,
    project_id: ProjectId,
    module: ModuleName,
) -> Result<Option<Arc<Vec<u8>>>, Error> {
    db.callbacks(project_id, module)
        .map(|op| Some(Arc::new(serde_json::to_vec(&op).unwrap())))
}
