// Standard library
use std::cell::RefCell;
use std::collections::{HashMap, hash_map::Entry};
use std::mem::take;
use std::sync::{Arc, Mutex, Weak};

// This crate
use crate::stt::{SttPoolItem, SttSet};

// Other crates
use anyhow::{anyhow, Result};
use log::debug;


thread_local!{
    pub static CAPS_MANAGER: RefCell<CapsManager> = RefCell::new(CapsManager::new());
}

/*** Session*******************************************************************/
pub struct SessionManager {
    sessions: HashMap<String, Arc<Mutex<Session>>>
}

// Session
impl SessionManager {
    pub fn new() -> Self {
        Self {sessions: HashMap::new()}
    }

    pub fn session_for(&mut self, uuid: String) -> Weak<Mutex<Session>> {
        match self.sessions.entry(uuid.clone()) {
            Entry::Occupied(o) => {
                Arc::downgrade(o.get())
            }
            Entry::Vacant(v) => {
                let arc = Arc::new(Mutex::new(Session::new(uuid)));
                Arc::downgrade(v.insert(arc))
            }
        }
    }

    pub fn end_session(&mut self, uuid: &str) -> Result<()> {
        match self.sessions.remove(uuid)  {
            Some(_) => {Ok(())}
            None => {Err(anyhow!("{} had no active session", uuid))}
        }
    }
}

pub struct Session {
    device: String,
    curr_utt: Option<SttPoolItem>
}

impl Session {
    fn new(device: String) -> Self {
        Self {device, curr_utt: None}
    }

    pub async fn get_stt_or_make(&mut self, set: &mut SttSet, audio: &[i16]) -> Result<&mut SttPoolItem> {
        match self.curr_utt {
            Some(ref mut i) => {
                Ok(i)
            }
            None => {
                let mut stt = set.guess_stt(audio).await?;
                debug!("STT for current session: {}", stt.get_info());
                stt.begin_decoding().await?;
                self.curr_utt = Some(stt);
                Ok(self.curr_utt.as_mut().unwrap())
            }
        }
        
    }

    pub fn end_utt(&mut self) -> Result<()> {
        match take(&mut self.curr_utt)  {
            Some(_) => {Ok(())}
            None => {Err(anyhow!("{} had no active session", &self.device))}
        }
    }
}

/*** Capabilities *************************************************************/
pub struct CapsManager {
    // For now just a map of capabilities, which is a map in which if exists is true
    clients_caps: HashMap<String, HashMap<String,()>>
}

// Capabilities
impl CapsManager {
    fn new() -> Self {
        Self {
            clients_caps: HashMap::new(),
        }
    }

    pub fn add_client(&mut self, uuid: &str, caps: Vec<String>) {
        let mut caps_map = HashMap::new();
        for cap in caps {
            caps_map.insert(cap, ());
        }
        
        self.clients_caps.insert(uuid.to_owned(), caps_map);
    }

    pub fn has_cap(&self, uuid: &str, cap_name: &str) -> bool {
        match self.clients_caps.get(uuid) {
            Some(client) => client.get(cap_name).map(|_|true).unwrap_or(false),
            None => false
        }
        
    }

    pub fn disconnected(&mut self, uuid: &str) -> Result<()> {
        match self.clients_caps.remove(uuid) {
            Some(_) => Ok(()),
            None => Err(anyhow!(format!("Satellite {} asked for a disconnect but was not connected", uuid)))
        }

    }

}