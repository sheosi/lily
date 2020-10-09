mod actions;
mod audio;
mod config;
mod hotword;
mod interfaces;
mod nlu;
mod path_ext;
mod python;
mod packages;
mod signals;
mod stt;
mod tts;
mod vad;
mod vars;

// Standard library
use std::path::Path;
use std::rc::Rc;

// This crate
use crate::config::get_conf;
use crate::packages::load_packages;
use crate::python::python_init;
use crate::vars::PACKAGES_PATH;

// Other crates
use anyhow::Result;
use pyo3::{conversion::IntoPy, Python, types::PyDict};
use unic_langid::LanguageIdentifier;


fn init_log() {
    let formatter = syslog::Formatter3164 {
        facility: syslog::Facility::LOG_USER,
        hostname: None,
        process: "lily".into(),
        pid: 0,
    };

    // Use Debug log level for debug compilations
    let log_level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    }
    else {
        log::LevelFilter::Info
    };

    let logger = syslog::unix(formatter).expect("could not connect to syslog");
    log::set_boxed_logger(Box::new(syslog::BasicLogger::new(logger)))
            .map(|()| log::set_max_level(log_level)).ok();
    //simple_logger::init()?;

}

fn get_locale_default() -> String {
    for (tag, val) in locale_config::Locale::user_default().tags() {
        if let None = tag {
            return format!("{}", val)
        }
    }

    "".to_string()
}

fn main() -> Result<()> {
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

    sigreg.call_loop("order", &config, &base_context, &curr_lang)?;

    Ok(())
}