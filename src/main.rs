mod actions;
mod config;
mod interfaces;
mod nlu;
mod path_ext;
mod python;
mod packages;
mod signals;
mod stt;
mod tts;
mod vars;

// Standard library
use std::path::Path;
use std::rc::Rc;

// This crate
use crate::config::get_conf;
use crate::packages::load_packages;
use crate::python::python_init;

// Other crates
use anyhow::Result;
use lily_common::other::init_log;
use lily_common::vars::PACKAGES_PATH;
use pyo3::{conversion::IntoPy, Python, types::PyDict};
use unic_langid::LanguageIdentifier;



fn get_locale_default() -> String {
    for (tag, val) in locale_config::Locale::user_default().tags() {
        if let None = tag {
            return format!("{}", val)
        }
    }

    "".to_string()
}

#[tokio::main]
pub async fn main()  -> Result<()> {
    // Set explicit handle for Ctrl-C signal
    ctrlc::set_handler(move || {
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");

    init_log();
    python_init()?;

    // Set config on global
    let config = get_conf();
    crate::config::GLOBAL_CONF.with(|c|c.replace(Rc::new(config.clone())));

    // Show config on debug
    log::debug!("{:?}", config);

    // 
    let curr_lang : LanguageIdentifier = {
        let as_str =
            if let Some(ref lang) =  config.language {
                lang.clone()
            }
            else {
                get_locale_default()
            };

        as_str.parse().expect("Locale parsing failed")
    };
    {
        let gil = Python::acquire_gil();
        let py = gil.python();

        crate::python::set_python_locale(py, &curr_lang)?;
    }

    let mut sigreg = load_packages(&Path::new(&PACKAGES_PATH.resolve()), &curr_lang)?;

    let base_context = {
        let gil = Python::acquire_gil();
        let py = gil.python();

        PyDict::new(py).into_py(py)
    };

    sigreg.call_loop("order", &config, &base_context, &curr_lang).await?;

    Ok(())
}