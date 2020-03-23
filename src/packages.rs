// Standard library
use std::path::Path;
use std::rc::Rc;
use std::collections::HashMap;

// This crate
use crate::vars::{resolve_path, NLU_TRAIN_SET_PATH, NLU_ENGINE_PATH, PYTHON_SDK_PATH, PACKAGES_PATH_ERR_MSG, WRONG_YAML_ROOT_MSG, WRONG_YAML_KEY_MSG, WRONG_YAML_SECTION_TYPE_MSG};
use crate::nlu::{NluManager, NluFactory, NluUtterance, EntityDef, EntityInstance};
use crate::python::{try_translate_all, PYTHON_LILY_PKG_NONE, PYTHON_LILY_PKG_CURR};
use crate::extensions::{OrderMap, ActionSet, ActionRegistry};

// Other crates
use unic_langid::LanguageIdentifier;
use log::{info, warn};
use cpython::Python;
use ref_thread_local::RefThreadLocal;
use anyhow::{anyhow, Result};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(untagged)]
enum OrderKind {
    Ref(String),
    Def(EntityDef)
}

#[derive(Deserialize)]
struct OrderEntity {
    kind: OrderKind,
    example: String
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OrderData {
    Direct(String),
    WithEntities{text: String, entities: HashMap<String, OrderEntity>}
}


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

    let canon_path = pkg_path.canonicalize()?;

    let pkg_name = {
        let os_str = canon_path.file_name().ok_or_else(||anyhow!("Can't get package path's name"))?;
        let pkg_name_str = os_str.to_str().ok_or_else(||anyhow!("Can't transform package path name to str"))?;
        Rc::new(pkg_name_str.to_string())
    };

    std::env::set_current_dir(canon_path)?;

    
    *PYTHON_LILY_PKG_CURR.borrow_mut() = pkg_name;
    lily_py_mod.call(python, "__set_translations", (curr_lang.to_string(),), None).map_err(|py_err|anyhow!("Python error while calling __set_translations: {:?}", py_err))?;
    *PYTHON_LILY_PKG_CURR.borrow_mut() = PYTHON_LILY_PKG_NONE.borrow().clone();

    Ok(())
}

pub fn load_package<N: NluManager>(order_map: &mut OrderMap, nlu_man: &mut N, action_registry: &ActionRegistry, path: &Path, _curr_lang: &LanguageIdentifier) -> Result<()> {
    info!("Loading package: {}", path.to_str().ok_or_else(|| anyhow!("Failed to get the str from path {:?}", path))?);

    // Set current Python module
    let pkg_name =  {
        let os_str = path.file_name().ok_or_else(||anyhow!("Can't get package path's name"))?;
        let pkg_name_str = os_str.to_str().ok_or_else(||anyhow!("Can't transform package path name to str"))?;
        Rc::new(pkg_name_str.to_string())
    };
    let pkg_path = Rc::new(path.to_path_buf());
    *crate::python::PYTHON_LILY_PKG_CURR.borrow_mut() = pkg_name.clone();

    let yaml_path = path.join("skills_def.yaml");
    if yaml_path.is_file() {
        // Load Yaml
        let doc:serde_yaml::Value = {
            let yaml_str = &std::fs::read_to_string(&yaml_path)?;
            serde_yaml::from_str(yaml_str)?
        };

        // Multi document support, doc is a yaml::YamlLoader, we just want the first one
        //let doc = &docs[0];

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
                    act_set.borrow_mut().add_action(py, &act_name, &act_arg, &action_registry, pkg_name.clone(), pkg_path.clone())?;
                }
                for (sig_name, sig_arg) in signals.into_iter() {

                    if sig_name == "order" {
                        let ord_data: OrderData = serde_yaml::from_value(sig_arg)?;
                        match ord_data {
                            OrderData::Direct(order_str) => {
                                // into_iter lets us do a move operation
                                let utts = try_translate_all(&order_str)?.into_iter().map(|utt| NluUtterance::Direct(utt)).collect();
                                nlu_man.add_intent(skill_name, utts);
                            }
                            OrderData::WithEntities{text, entities} => {

                                let mut entities_res = HashMap::new();
                                for (ent_name, ent_data) in entities.into_iter() {
                                    let ent_kind_name = match ent_data.kind {
                                        OrderKind::Ref(name) => {
                                            name
                                        },
                                        OrderKind::Def(def) => {
                                            let name = format!("_{}__{}_", pkg_name, ent_name);
                                            nlu_man.add_entity(&name, def);
                                            name
                                        }
                                    };
                                    entities_res.insert(ent_name, EntityInstance{kind:ent_kind_name, example:ent_data.example});
                                }
                                let utts = try_translate_all(&text)?.into_iter().map(|utt| NluUtterance::WithEntities{text:utt, entities: entities_res.clone()}).collect();
                                nlu_man.add_intent(skill_name, utts);
                            }
                        }
                    }
                    else {
                        warn!("Unknown signal {} present in conf file", sig_name);
                    }
                }

                order_map.add_order(skill_name, act_set);
            }
            else {
                warn!("Incorrect Yaml format for skill: {}, won't be loaded", key.clone().as_str().expect(WRONG_YAML_KEY_MSG));
            }
        }
    }
    *crate::python::PYTHON_LILY_PKG_CURR.borrow_mut() = crate::python::PYTHON_LILY_PKG_NONE.borrow().clone();

    Ok(())
}

pub fn load_packages(path: &Path, curr_lang: &LanguageIdentifier) -> Result<OrderMap> {
    let mut order_map = OrderMap::new();
    let mut nlu_man = NluFactory::new_manager();

    // Get GIL
    let gil = Python::acquire_gil();
    let py = gil.python();

    let action_registry = ActionRegistry::new_with_no_trans(py, &resolve_path(PYTHON_SDK_PATH))?;


    info!("PACKAGES_PATH:{}", path.to_str().ok_or_else(|| anyhow!("Can't transform the package path {:?}", path))?);
    for entry in std::fs::read_dir(path).expect(PACKAGES_PATH_ERR_MSG) {
        let entry = entry?.path();
        if entry.is_dir() {
            load_trans(py, &entry, curr_lang)?;
            load_package(&mut order_map, &mut nlu_man, &action_registry.clone_try_adding(py, &entry.join("python"))?, &entry, curr_lang)?;
        }

        info!("{:?}", action_registry);
    }

    nlu_man.train(&resolve_path(NLU_TRAIN_SET_PATH), &resolve_path(NLU_ENGINE_PATH), curr_lang)?;

    Ok(order_map)
}