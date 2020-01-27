// Standard library
use std::path::Path;
use std::rc::Rc;

// This crate
use crate::vars::{resolve_path, NLU_TRAIN_SET_PATH, NLU_ENGINE_PATH, PYTHON_SDK_PATH, PACKAGES_PATH_ERR_MSG, WRONG_YAML_ROOT_MSG, WRONG_YAML_KEY_MSG, WRONG_YAML_SECTION_TYPE_MSG};
use crate::nlu::NluManager;
use crate::python::try_translate;
use crate::extensions::{OrderMap, ActionSet, ActionRegistry};

// Other crates
use yaml_rust::yaml::{YamlLoader, Hash};
use unic_langid::LanguageIdentifier;
use log::{info, warn};
use cpython::Python;
use ref_thread_local::RefThreadLocal;
use anyhow::{anyhow, Result};

pub fn load_package(order_map: &mut OrderMap, nlu_man: &mut NluManager, action_registry: &ActionRegistry, path: &Path, _curr_lang: &LanguageIdentifier) -> Result<()> {
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
        let docs = {
            let yaml_str = &std::fs::read_to_string(&yaml_path)?;
            YamlLoader::load_from_str(yaml_str)?
        };

        // Multi document support, doc is a yaml::YamlLoader
        let doc = &docs[0];

        //Debug support
        println!("{:?}", docs);

        // Load actions + singals from Yaml
        for (key, data) in doc.as_hash().expect(WRONG_YAML_ROOT_MSG).iter() {
            if let Some(skill_def) = data.as_hash() {
                let skill_name = key.as_str().expect(WRONG_YAML_KEY_MSG);

                fn parse_skills_sections<'a>(yaml_hash: &'a Hash) -> (Vec<(String, &'a yaml_rust::Yaml)>, Vec<(String, &'a yaml_rust::Yaml)>) {
                    let mut actions: Vec<(String, &yaml_rust::Yaml)> = Vec::new();
                    let mut signals: Vec<(String, &yaml_rust::Yaml)> = Vec::new();

                    for (sec_name, sec_data) in yaml_hash.iter() {
                        let as_str = sec_name.clone().into_string().expect(WRONG_YAML_KEY_MSG);
                        
                        match as_str.as_str() {
                            "actions" => {
                                for (key3, data3) in sec_data.as_hash().expect(WRONG_YAML_SECTION_TYPE_MSG).iter() {
                                    actions.push((key3.clone().into_string().expect(WRONG_YAML_KEY_MSG), data3));
                                }
                            }
                            "signals" => {
                                for (key3, data3) in sec_data.as_hash().expect(WRONG_YAML_SECTION_TYPE_MSG).iter() {
                                    signals.push((key3.clone().into_string().expect(WRONG_YAML_KEY_MSG), data3));
                                }
                            }
                            _ => {}
                        }

                    }
                    (actions, signals)
                }

                let (actions, signals) = parse_skills_sections(skill_def);

                let act_set = ActionSet::create();
                for (act_name, act_arg) in actions.iter() {
                    let gil = Python::acquire_gil();
                    let py = gil.python();
                    act_set.borrow_mut().add_action(py, act_name, act_arg, &action_registry, pkg_name.clone(), pkg_path.clone())?;
                }
                for (sig_name, sig_arg) in signals.iter() {

                    if sig_name == &"order" {
                        if let Some(order_str) = sig_arg.as_str() {
                            nlu_man.add_intent(skill_name, vec![try_translate(order_str)?]);
                        }
                        else {
                            warn!("Order's arg is not a string, can't be understood");
                        }
                    }
                    else {
                        warn!("Unknown signal {} present in conf file", sig_name);
                    }
                }

                order_map.add_order(skill_name, act_set);
            }
            else {
                warn!("Incorrect Yaml format for skill: {}, won't be loaded", key.clone().into_string().expect(WRONG_YAML_KEY_MSG));
            }
        }
    }
    *crate::python::PYTHON_LILY_PKG_CURR.borrow_mut() = crate::python::PYTHON_LILY_PKG_NONE.borrow().clone();

    Ok(())
}

pub fn load_packages(path: &Path, curr_lang: &LanguageIdentifier) -> Result<OrderMap> {
    let mut order_map = OrderMap::new();
    let mut nlu_man = NluManager::new();

    // Get GIL
    let gil = Python::acquire_gil();
    let py = gil.python();

    let action_registry = ActionRegistry::new_with_no_trans(py, &resolve_path(PYTHON_SDK_PATH))?;


    info!("PACKAGES_PATH:{}", path.to_str().ok_or_else(|| anyhow!("Can't transform the package path {:?}", path))?);
    for entry in std::fs::read_dir(path).expect(PACKAGES_PATH_ERR_MSG) {
        let entry = entry?.path();
        if entry.is_dir() {
            load_package(&mut order_map, &mut nlu_man, &action_registry.clone_try_adding(py, &entry.join("python"), curr_lang)?, &entry, curr_lang)?;
        }
    }

    nlu_man.train(&resolve_path(NLU_TRAIN_SET_PATH), &resolve_path(NLU_ENGINE_PATH), curr_lang)?;

    Ok(order_map)
}