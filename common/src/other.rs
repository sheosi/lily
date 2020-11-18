use serde::{Deserialize, Serialize};

pub fn init_log(name: String) {
    let formatter = syslog::Formatter3164 {
        facility: syslog::Facility::LOG_USER,
        hostname: None,
        process: name,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConnectionConf {
    #[serde(default = "ConnectionConf::def_url_str")]
    #[serde(skip_serializing_if = "ConnectionConf::is_def_url_str")]
    pub url_str: String,

    #[serde(default = "ConnectionConf::def_name")]
    pub name: Option<String>,

    #[serde(default = "ConnectionConf::def_user_pass")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_pass: Option<(String, String)>
}
impl ConnectionConf {
    fn def_url_str() -> String {
        "localhost".into()
    }

    fn is_def_url_str(input: &str) -> bool {
        input == Self::def_url_str()
    }

    fn def_name() -> Option<String> {
        None
    }

    fn def_user_pass() -> Option<(String, String)> {
        None
    }
}

impl Default for ConnectionConf {
    fn default() -> Self {
        Self {
            url_str: Self::def_url_str(),
            name: Self::def_name(),
            user_pass: Self::def_user_pass()
        }
    }
}

pub fn none<T>()-> Option<T> {
    None
}

pub fn false_val() -> bool {
    false
}