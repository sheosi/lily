// Standard library
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

// This crate
use crate::actions::{ActionSet, ActionRegistry, ActionRegistryShared, LocalActionRegistry};
use crate::python::{add_py_folder, call_for_pkg, remove_from_actions, remove_from_signals};
use crate::signals::{LocalSignalRegistry, new_signal_order, PythonSignal, SignalRegistry, SignalRegistryShared, Timer, PollQuery};
use crate::vars::{PACKAGES_PATH_ERR_MSG, POISON_MSG, PYTHON_SDK_PATH, WRONG_YAML_ROOT_MSG, WRONG_YAML_KEY_MSG, WRONG_YAML_SECTION_TYPE_MSG};

// Other crates
use anyhow::{anyhow, Result};
use pyo3::Python;
use thiserror::Error;
use log::{error, info, warn};
use unic_langid::LanguageIdentifier;

trait IntoMapping {
    fn into_mapping(self) -> Option<serde_yaml::Mapping>;
}

impl IntoMapping for serde_yaml::Value {
    fn into_mapping(self) -> Option<serde_yaml::Mapping> {
        match self {
            serde_yaml::Value::Mapping(mapping) => Some(mapping),
            _ => None
        }
    }
}

fn load_trans(python: Python, pkg_path: &Path, curr_langs: &Vec<LanguageIdentifier>) -> Result<()>{
    let lily_py_mod = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;

    let as_strs: Vec<String> = curr_langs.into_iter().map(|i|i.to_string()).collect();
    call_for_pkg(pkg_path, |_| lily_py_mod.call("__set_translations", (as_strs,), None).map_err(|py_err|anyhow!("Python error while calling __set_translations: {:?}", py_err)))??;

    Ok(())
}

pub fn load_skills(sigreg: &mut LocalSignalRegistry, action_registry: &LocalActionRegistry, path: &Path) -> Result<()> {
    info!("Loading package: {}", path.to_str().ok_or_else(|| anyhow!("Failed to get the str from path {:?}", path))?);

    let pkg_path = Arc::new(path.to_path_buf());
    call_for_pkg::<_, Result<()>>(path, |pkg_name|{

        let yaml_path = path.join("skills_def.yaml");
        if yaml_path.is_file() {
            // Load Yaml
            let doc:serde_yaml::Value = {
                let yaml_str = &std::fs::read_to_string(&yaml_path)?;
                serde_yaml::from_str(yaml_str)?
            };

            // Load actions + singals from Yaml
            for (key, data) in doc.into_mapping().expect(WRONG_YAML_ROOT_MSG).into_iter() {
                if let Some(skill_def) = data.into_mapping() {
                    let skill_name = key.as_str().expect(WRONG_YAML_KEY_MSG);

                    fn parse_skills_sections<'a>(yaml_hash: serde_yaml::Mapping) -> (Vec<(String, serde_yaml::Value)>, Vec<(String, serde_yaml::Value)>) {
                        let mut actions: Vec<(String, serde_yaml::Value)> = Vec::new();
                        let mut signals: Vec<(String, serde_yaml::Value)> = Vec::new();

                        for (sec_name, sec_data) in yaml_hash.into_iter() {
                            let as_str = sec_name.as_str().expect(WRONG_YAML_KEY_MSG);
                            
                            match as_str {
                                "actions" => {
                                    for (key3, data3) in sec_data.into_mapping().expect(WRONG_YAML_SECTION_TYPE_MSG).into_iter() {
                                        actions.push((key3.clone().as_str().expect(WRONG_YAML_KEY_MSG).to_string(), data3));
                                    }
                                }
                                "signals" => {
                                    for (key3, data3) in sec_data.into_mapping().expect(WRONG_YAML_SECTION_TYPE_MSG).into_iter() {
                                        signals.push((key3.clone().as_str().expect(WRONG_YAML_KEY_MSG).to_string(), data3));
                                    }
                                }
                                _ => {}
                            }

                        }
                        (actions, signals)
                    }

                    let (actions, signals) = parse_skills_sections(skill_def);

                    let act_set = ActionSet::create();
                    for (act_name, act_arg) in actions.into_iter() {
                        let act = action_registry.get(&act_name).ok_or_else(||anyhow!("Action {} is not registered",act_name))?;
                        let act_inst = act.borrow().instance(&act_arg, pkg_path.clone());
                        act_set.lock().expect(POISON_MSG).add_action(act_inst)?;
                    }


                    for (sig_name, sig_arg) in signals.into_iter() {
                        if let Err(e) = sigreg.add_sigact_rel(&sig_name, sig_arg, skill_name, &pkg_name, act_set.clone()) {
                            return Err(anyhow!("Skill \"{}\" that refers to a signal \"{}\" which is inexistent or not available to the package. More info: {}", skill_name, sig_name, e))
                        }
                    }

                }
                else {
                    return Err(anyhow!("Incorrect Yaml format for skill: \"{}\", won't be loaded", key.clone().as_str().expect(WRONG_YAML_KEY_MSG)))    
                }
            }
        }
        Ok(())
    })??;

    Ok(())
}


/** Used by other modules, launched after an error while loading classes, 
contains the error and the new classes*/
#[derive(Debug, Error)]
#[error("{source}")]
pub struct HalfBakedDoubleError {
    pub act_sig: (LocalSignalRegistry, LocalActionRegistry),

    pub source: anyhow::Error,
}

impl HalfBakedDoubleError {
    fn from(act_sig:(LocalSignalRegistry, LocalActionRegistry), source: anyhow::Error ) -> Self {
        Self {act_sig, source}
    }
}


pub fn load_packages(path: &Path, curr_lang: &Vec<LanguageIdentifier>) -> Result<SignalRegistry> {
    let loaders: Vec<Box<dyn Loader>> = vec![
        Box::new(PythonLoader::new()),
        Box::new(EmbeddedLoader::new())
    ];

    let global_sigreg = Rc::new(RefCell::new(SignalRegistry::new()));
    let mut base_sigreg = LocalSignalRegistry::new(global_sigreg.clone());

    let global_actreg = Rc::new(RefCell::new(ActionRegistry::new()));
    let mut base_actreg = LocalActionRegistry::new(global_actreg.clone());

    for loader in &loaders {
        let (ldr_sigreg, ldr_actreg) = 
            loader.init_base(global_sigreg.clone(), global_actreg.clone(), curr_lang.to_owned())?;

        base_sigreg.extend(ldr_sigreg);
        base_actreg.extend(ldr_actreg);
    }

    info!("PACKAGES_PATH:{}", path.to_str().ok_or_else(|| anyhow!("Can't transform the package path {:?}", path))?);

    let mut not_loaded = vec![];
    

    let process_pkg = |entry: &Path| -> Result<(), HalfBakedDoubleError> {
        let mut pkg_sigreg = base_sigreg.clone();
        let mut pkg_actreg = base_actreg.clone();
        for loader in &loaders {
            let (local_sigreg, local_actreg)= 
                loader.load_code(&entry, &pkg_sigreg, &pkg_actreg).map_err(|e|{
                    HalfBakedDoubleError::from((pkg_sigreg, pkg_actreg), e)
                })?;

                pkg_sigreg = local_sigreg;
                pkg_actreg = local_actreg;
        }
        {
            // Get GIL
            let gil = Python::acquire_gil();
            let py = gil.python();

            load_trans(py, &entry, curr_lang).map_err(|e|{
                HalfBakedDoubleError::from((pkg_sigreg.clone(), pkg_actreg.clone()), e)
            })?;
        }
        load_skills(&mut pkg_sigreg, &pkg_actreg, &entry).map_err(|e|{
            HalfBakedDoubleError::from((pkg_sigreg, pkg_actreg), e)
        })?;
        Ok(())
    };

    for entry in std::fs::read_dir(path).expect(PACKAGES_PATH_ERR_MSG) {
        let entry = entry?.path();
        if entry.is_dir() {
            match process_pkg(&entry) {
                Err(e) => {
                    let (pkg_sigreg, pkg_actreg) = e.act_sig;
                    pkg_sigreg.minus(&base_sigreg).remove_from_global();
                    pkg_actreg.minus(&base_actreg).remove_from_global();
                    let pkg_name = entry.file_stem().expect("Couldn't get stem from file").to_string_lossy();
                    error!("Package {} had a problem, won't be available. {}", pkg_name, e.source);
                    not_loaded.push(pkg_name.into_owned());
                },
                _ => ()
            }
        }
    }

    if !not_loaded.is_empty() {
        warn!("Not loaded: {}", not_loaded.join(","));
    }

    global_sigreg.borrow_mut().end_load(curr_lang)?;

    // This is overall stupid but haven't found any other (interesting way to do it)
    // We need the variable to help lifetime analisys
    let res = global_sigreg.borrow_mut().clone();
    Ok(res)
}

trait Loader {
    fn init_base(&self, glob_sigreg: SignalRegistryShared, glob_actreg: ActionRegistryShared, lang: Vec<LanguageIdentifier>) -> Result<(LocalSignalRegistry, LocalActionRegistry)>;
    fn load_code(&self, pkg_path: &Path, base_sigreg: &LocalSignalRegistry, base_actreg: &LocalActionRegistry) -> Result<(LocalSignalRegistry, LocalActionRegistry)> ;
}

struct PythonLoader {}

impl PythonLoader {
    pub fn load_package_python(py: Python, path: &Path,
        pkg_path: &Path,
        base_sigreg: &LocalSignalRegistry,
        base_actreg: &LocalActionRegistry
    ) -> Result<(LocalSignalRegistry, LocalActionRegistry)> {

        let (signal_classes, action_classes) = add_py_folder(py, path)?;
        let mut sigreg = base_sigreg.clone();
        match PythonSignal::extend_and_init_classes_py_local(&mut sigreg, py, pkg_path, signal_classes) {
            Ok(()) =>{Ok(())}
            Err(e) => {
                remove_from_signals(py, &e.cls_names).expect(&format!("Coudln't remove '{:?}' from signals", e.cls_names));
                Err(e.source)
            }
        }?;

        let mut actreg = base_actreg.clone();
        match actreg.extend_and_init_classes(py, action_classes) {
            Ok(()) => {Ok(())}
            Err(e) => {
                remove_from_actions(py, &e.cls_names).expect(&format!("Couldn't remove '{:?}' from actions", e.cls_names));
                sigreg.minus(base_sigreg).remove_from_global();
                Err(e.source)
            }
        }?;

        Ok((sigreg, actreg))
    }
}

impl Loader for PythonLoader {
    fn init_base(&self, glob_sigreg: SignalRegistryShared, glob_actreg: ActionRegistryShared, _lang: Vec<LanguageIdentifier>) -> Result<(LocalSignalRegistry, LocalActionRegistry)> {
        // Get GIL
        let gil = Python::acquire_gil();
        let py = gil.python();

        let path = &PYTHON_SDK_PATH.resolve();
        let (initial_signals, initial_actions) = add_py_folder(py, &PYTHON_SDK_PATH.resolve())?;
        let mut sigreg = LocalSignalRegistry::new(glob_sigreg);
        PythonSignal::extend_and_init_classes_py_local(&mut sigreg, py, path, initial_signals)?;

        let mut actreg =LocalActionRegistry::new(glob_actreg);
        actreg.extend_and_init_classes(py, initial_actions)?;

        Ok((sigreg, actreg))
    }

    fn load_code(&self, pkg_path: &Path, base_sigreg: &LocalSignalRegistry, base_actreg: &LocalActionRegistry) -> Result<(LocalSignalRegistry, LocalActionRegistry)> {
        // Get GIL
        let gil = Python::acquire_gil();
        let py = gil.python();
        Self::load_package_python(py, &pkg_path.join("python"), &pkg_path, base_sigreg, base_actreg)
    }
}
impl PythonLoader {
    fn new() -> Self {
        Self{}
    }
}

struct EmbeddedLoader {
}

impl EmbeddedLoader {
    fn new() -> Self {
        Self{}
    }
}

impl Loader for EmbeddedLoader {
    fn init_base(&self, glob_sigreg: SignalRegistryShared, glob_actreg: ActionRegistryShared, lang: Vec<LanguageIdentifier>) -> Result<(LocalSignalRegistry, LocalActionRegistry)> {
        let mut sigreg = LocalSignalRegistry::new(glob_sigreg);
        sigreg.insert("order".into(), Rc::new(RefCell::new(new_signal_order(lang))))?;
        sigreg.insert("timer".into(), Rc::new(RefCell::new(Timer::new())))?;
        sigreg.insert("private__poll_query".into(), Rc::new(RefCell::new(PollQuery::new())))?;

        Ok((sigreg, LocalActionRegistry::new(glob_actreg)))
    }

    fn load_code(&self, _pkg_path: &Path, base_sigreg: &LocalSignalRegistry, base_actreg: &LocalActionRegistry) -> Result<(LocalSignalRegistry, LocalActionRegistry)> {
        Ok((base_sigreg.clone(), base_actreg.clone()))
    }
}