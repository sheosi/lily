mod stt;
mod tts;
mod audio;
mod nlu;
mod vars;
mod hotword;
mod python;
mod packages;
mod vad;
mod actions;
mod config;
mod path_ext;
mod signals;

// Standard library
use std::path::Path;

// This crate
use crate::vars::PACKAGES_PATH;
use crate::packages::load_packages;
use crate::python::python_init;
use crate::config::get_conf;

// Other crates
use unic_langid::LanguageIdentifier;
use anyhow::Result;
use cpython::PyDict;


fn init_log() {
    let formatter = syslog::Formatter3164 {
        facility: syslog::Facility::LOG_USER,
        hostname: None,
        process: "lily".into(),
        pid: 0,
    };


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
    ctrlc::set_handler(move || {
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");

    init_log();
    python_init()?;

    // Set language
    let config = get_conf();
    println!("{:?}", config);
    let curr_lang : LanguageIdentifier = get_locale_default().parse().expect("Locale parsing failed");
    {
        let gil = cpython::Python::acquire_gil();
        let py = gil.python();

        crate::python::set_python_locale(py, &curr_lang)?;
    }

    let (mut signal_order, mut signal_event) = load_packages(&Path::new(&PACKAGES_PATH.resolve()), &curr_lang)?;

    let base_context = {
        let gil = cpython::Python::acquire_gil();
        let py = gil.python();

        PyDict::new(py)
    };


    signal_order.record_loop(&mut signal_event, &config, &base_context, &curr_lang)?;

    Ok(())
}