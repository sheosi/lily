mod actions;
mod config;
mod nlu;
mod exts;
mod mqtt;
#[cfg(feature="python_skills")]
mod python;
mod skills;
mod collections;
mod queries;
mod signals;
mod stt;
#[cfg(test)]
mod tests;
mod tts;
mod vars;

// Standard library
use std::rc::Rc;


// This crate
use crate::actions::DynamicDict;
use crate::config::Config;
use crate::exts::LockIt;
use crate::skills::load_skills;
#[cfg(feature="python_skills")]
use crate::python::{python_init, set_python_locale};
use crate::signals::{dynamic_nlu::init_dynamic_nlu, SIG_REG};
use crate::vars::SKILLS_PATH;

// Other crates
use anyhow::Result;
use lily_common::other::init_log;
use lily_common::vars::set_app_name;
use unic_langid::LanguageIdentifier;

#[cfg(feature="python_skills")]
use pyo3::Python;

fn get_locale_default() -> String {
    for (tag, val) in locale_config::Locale::user_default().tags() {
        if tag.is_none() {
            return format!("{}", val)
        }
    }

    "".to_string()
}

#[cfg(feature="python_skills")]
fn set_py_locale(lang_id: &LanguageIdentifier) -> Result<()> {
    let gil = Python::acquire_gil();
    let py = gil.python();

    set_python_locale(py, lang_id)
}

#[cfg(not(feature="python_skills"))]
fn set_py_locale(_lang_id: &LanguageIdentifier) -> Result<()> {
    Ok(())
}

#[cfg(not(feature="python_skills"))]
fn python_init()-> Result<()> {
    Ok(())
}

#[tokio::main(flavor="current_thread")]
pub async fn main()  -> Result<()> {
    // Set explicit handle for Ctrl-C signal
    ctrlc::set_handler(move || {
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");

    set_app_name("lily");
    init_log("lily".into());

    if cfg!(feature = "python_skills") {
        python_init()?;
    }

    // Set config on global
    let config = Config::load().unwrap_or_default();
    crate::config::GLOBAL_CONF.with(|c|c.replace(Rc::new(config.clone())));

    // Show config on debug
    log::debug!("{:?}", config);

    // 
    let curr_langs : Vec<LanguageIdentifier> = {
        let as_str =
            if let Some(ref lang) =  config.language {
                lang.clone()
            }
            else {
                vec![get_locale_default()]
            };

        as_str.into_iter().map(|i|i.parse().expect("Locale parsing failed")).collect()
    };

    if cfg!(feature = "python_skills") {
        set_py_locale(&curr_langs[0])?;
    }

    let consumer = init_dynamic_nlu()?;

    load_skills(SKILLS_PATH.all(), &curr_langs, consumer)?;

    //TODO!: This can very well be problematic since we access it later too.
    SIG_REG.lock_it().call_loops(&config, &DynamicDict::new(), &curr_langs).await?;

    Ok(())
}