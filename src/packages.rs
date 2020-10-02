// Standard library
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

// This crate
use crate::vars::{PYTHON_SDK_PATH, PACKAGES_PATH_ERR_MSG, WRONG_YAML_ROOT_MSG, WRONG_YAML_KEY_MSG, WRONG_YAML_SECTION_TYPE_MSG};
use crate::python::{add_py_folder, call_for_pkg};
use crate::actions::{ActionSet, ActionRegistry, LocalActionRegistry};
use crate::signals::{LocalSignalRegistry, SignalRegistry};

// Other crates
use anyhow::{anyhow, Result};
use cpython::Python;
use log::{info, warn};
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

fn load_trans(python: Python, pkg_path: &Path, curr_lang: &LanguageIdentifier) -> Result<()>{
    let lily_py_mod = python.import("lily_ext").map_err(|py_err|anyhow!("Python error while importing lily_ext: {:?}", py_err))?;

    call_for_pkg(pkg_path, |_| lily_py_mod.call(python, "__set_translations", (curr_lang.to_string(),), None).map_err(|py_err|anyhow!("Python error while calling __set_translations: {:?}", py_err)))??;

    Ok(())
}

pub fn load_package(sigreg: &mut LocalSignalRegistry, action_registry: &LocalActionRegistry, path: &Path, _curr_lang: &LanguageIdentifier) -> Result<()> {
    // TODO: Don't load package if skills go wrong
    info!("Loading package: {}", path.to_str().ok_or_else(|| anyhow!("Failed to get the str from path {:?}", path))?);

    let pkg_path = Rc::new(path.to_path_buf());
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
                        let gil = Python::acquire_gil();
                        let py = gil.python();
                        act_set.borrow_mut().add_action(py, &act_name, &act_arg, &action_registry, pkg_path.clone())?;
                    }


                    for (sig_name, sig_arg) in signals.into_iter() {
                        if let Err(e) = sigreg.add_sigact_rel(&sig_name, sig_arg, skill_name, &pkg_name, act_set.clone()) {
                            log::warn!("Package \"{}\" with skill \"{}\" that refers to a signal \"{}\" which is inexistent or not available to the package, skill won't be loaded, error: {}", &pkg_name, skill_name, sig_name, e);
                        }
                    }

                }
                else {
                    warn!("Incorrect Yaml format for skill: \"{}\", won't be loaded", key.clone().as_str().expect(WRONG_YAML_KEY_MSG));
                }
            }
        }
        Ok(())
    })??;

    Ok(())
}

pub fn load_package_python(py: Python, path: &Path,
        pkg_path: &Path,
        base_sigreg: &LocalSignalRegistry,
        base_actreg: &LocalActionRegistry
    ) -> Result<(LocalSignalRegistry, LocalActionRegistry)> {
    let (signal_classes, action_classes) = add_py_folder(py, path)?;
    let mut sigreg = base_sigreg.clone();
    sigreg.extend_and_init_classes_py(py, pkg_path, signal_classes)?;

    let mut actreg = base_actreg.clone();
    actreg.extend_and_init_classes(py, action_classes)?;

    Ok((sigreg, actreg))
}

pub fn load_packages(path: &Path, curr_lang: &LanguageIdentifier) -> Result<SignalRegistry> {
    // Get GIL
    let gil = Python::acquire_gil();
    let py = gil.python();

    let (initial_signals, initial_actions) = add_py_folder(py, &PYTHON_SDK_PATH.resolve())?;

    let global_sigreg = Rc::new(RefCell::new(SignalRegistry::new()));
    let mut base_sigreg = LocalSignalRegistry::init_from(global_sigreg.clone());
    base_sigreg.extend_and_init_classes_py(py, path, initial_signals)?;

    let global_actreg = Rc::new(RefCell::new(ActionRegistry::new()));
    let mut base_actreg = LocalActionRegistry::new(global_actreg);
    base_actreg.extend_and_init_classes(py, initial_actions)?;

    info!("PACKAGES_PATH:{}", path.to_str().ok_or_else(|| anyhow!("Can't transform the package path {:?}", path))?);
    for entry in std::fs::read_dir(path).expect(PACKAGES_PATH_ERR_MSG) {
        let entry = entry?.path();
        if entry.is_dir() {
            let (mut local_sigreg, local_actreg) = load_package_python(py, &entry.join("python"), &entry, &base_sigreg, &base_actreg).unwrap();
            load_trans(py, &entry, curr_lang)?;
            load_package(&mut local_sigreg, &local_actreg, &entry, curr_lang)?;
        }
    }

    global_sigreg.borrow_mut().end_load(curr_lang)?;

    // This is overall stupid but haven't found any other (interesting way to do it)
    // We need the variable to help lifetime analisys
    let res = global_sigreg.borrow_mut().clone();
    Ok(res)
}