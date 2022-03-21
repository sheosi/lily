mod actions;
mod config;
mod nlu;
mod exts;
mod mqtt;
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
use crate::config::Config;
use crate::exts::LockIt;
use crate::skills::load_skills;
use crate::signals::SIG_REG;
use crate::vars::SKILLS_PATH;

// Other crates
use anyhow::Result;
use lily_common::other::init_log;
use lily_common::vars::set_app_name;
use unic_langid::LanguageIdentifier;

fn get_locale_default() -> String {
    for (tag, val) in locale_config::Locale::user_default().tags() {
        if tag.is_none() {
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

        as_str.into_iter().filter(|i|i.len()>0).map(|i|i.parse().expect(&format!("Locale parsing of \"{}\" failed",&i))).collect()
    };

    load_skills(SKILLS_PATH.all(), &curr_langs)?;

    //TODO!: This can very well be problematic since we access it later too.
    SIG_REG.lock_it().call_loops(&config, &curr_langs).await?;

    Ok(())
}