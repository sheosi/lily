mod actions;
mod config;
mod nlu;
mod exts;
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
use crate::actions::ActionContext;
use crate::config::Config;
use crate::skills::load_skills;
use crate::python::{python_init, set_python_locale};
use crate::signals::dynamic_nlu::init_dynamic_entities;
use crate::vars::SKILLS_PATH;

// Other crates
use anyhow::Result;
use lily_common::other::init_log;
use lily_common::vars::set_app_name;
use pyo3::Python;
use unic_langid::LanguageIdentifier;


fn get_locale_default() -> String {
    for (tag, val) in locale_config::Locale::user_default().tags() {
        if let None = tag {
            return format!("{}", val)
        }
    }

    "".to_string()
}

#[tokio::main(flavor="current_thread")]
pub async fn main()  -> Result<()> {
    // Set explicit handle for Ctrl-C signal
    ctrlc::set_handler(move || {
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");

    set_app_name("lily");
    init_log("lily".into());
    python_init()?;

    // Set config on global
    let config = Config::load().unwrap_or(Config::default());
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
    {
        let gil = Python::acquire_gil();
        let py = gil.python();

        set_python_locale(py, &curr_langs[0])?;
    }

    let consumer = init_dynamic_entities()?;

    let mut sigreg = load_skills(SKILLS_PATH.all(), &curr_langs, consumer)?;
    sigreg.call_loops(&config, &ActionContext::new(), &curr_langs).await?;

    Ok(())
}