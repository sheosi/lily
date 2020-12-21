use std::cell::RefCell;
use std::collections::HashMap;
use std::cmp::Ordering;
use std::ops::{Deref, DerefMut};
use std::rc::{Rc, Weak};

use crate::stt::{IbmSttData, Stt, SttError, SttFactory};

use anyhow::Result;
use unic_langid::LanguageIdentifier;


struct SttPoolData {
    items: Vec<Box<dyn Stt>>,
}

impl SttPoolData {
    async fn new(
        initial_size: u8,
        capacity: u8,
        lang: &LanguageIdentifier,
        prefer_online: bool,
        ibm_data: &Option<IbmSttData>
    ) -> Result<Self> {
        let mut items = Vec::with_capacity(capacity as usize);
        for _ in 0..initial_size {
            let stt = SttFactory::load(lang, prefer_online, ibm_data.clone()).await?;
            items.push(stt);
        }
        Ok(Self {items})
    }

    fn return_val(&mut self, value: Box<dyn Stt>) {
        if self.items.len() < self.items.capacity() {
            self.items.push(value);
        }
    }
}

pub struct SttPool {
    data: Rc<RefCell<SttPoolData>>,
    lang: LanguageIdentifier,
    prefer_online: bool,
    ibm_data: Option<IbmSttData>
}

impl SttPool {
    pub async fn new(
        initial_size: u8,
        capacity: u8,
        lang: &LanguageIdentifier,
        prefer_online: bool,
        ibm_data: &Option<IbmSttData>
    ) -> Result<Self> {
        Ok(Self {
            data: Rc::new(RefCell::new(SttPoolData::new(initial_size, capacity, lang, prefer_online, ibm_data).await?)),
            lang: lang.clone(),
            prefer_online,
            ibm_data: ibm_data.clone()
        })
    }

    async fn take(&mut self) -> Result<SttPoolItem> {
        Ok(SttPoolItem {
            pool: Rc::downgrade(&self.data),
            value: Some(SttFactory::load(&self.lang, self.prefer_online, self.ibm_data.clone()).await?),
            lang: self.lang.clone()
        })
    }


}

pub struct SttPoolItem {
    pool: Weak<RefCell<SttPoolData>>,
    lang: LanguageIdentifier,
    value: Option<Box<dyn Stt>>
}

impl Deref for SttPoolItem {
    type Target = Box<dyn Stt>;

    fn deref(&self) -> &Self::Target {
        &self.value.as_ref().expect("")
    }
}

impl DerefMut for SttPoolItem {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value.as_mut().unwrap()
    }
}

impl Drop for SttPoolItem {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.upgrade() {
            pool.borrow_mut().return_val(self.value.take().unwrap());
        }
    }
}

impl SttPoolItem {
    pub fn lang(&self) -> &LanguageIdentifier {&self.lang}
}

pub struct SttSet {
    map: HashMap<LanguageIdentifier, SttPool>,
    detector: LangDetector
}

impl SttSet {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            detector: LangDetector::new()
        }
    }

    pub async fn add_lang(&mut self, lang_id: &LanguageIdentifier, mut pool: SttPool) -> Result<()> {
        self.detector.add_lang(lang_id, pool.take().await?);
        self.map.insert(lang_id.clone(), pool);

        Ok(())
    }

    pub async fn get_session_for(&mut self, audio: &[i16]) -> Result<SttPoolItem> {
        let lang = self.detector.detect_lang(audio).await?;
        Ok(self.map.get_mut(&lang).unwrap().take().await?)
    }
}

struct LangDetector {
    stts: Vec<(LanguageIdentifier, SttPoolItem)>
}

impl LangDetector {
    fn new() -> Self {
        Self{stts: Vec::new()}
    }

    fn add_lang(&mut self, lang_id: &LanguageIdentifier, stt: SttPoolItem) {
        self.stts.push((lang_id.to_owned(), stt));
    }

    async fn detect_lang(&mut self, audio: &[i16]) -> Result<LanguageIdentifier, SttError> {
        async fn confidence_for(stt: &mut Box<dyn Stt>, audio_samp: &[i16]) -> Result<f32, SttError> {
            stt.begin_decoding().await?;
            stt.process(audio_samp).await?;
            let confidence = match stt.end_decoding().await? {
                Some(decode) => decode.confidence,
                None => 0.0
            };

            Ok(confidence)
        }


        let mut results = Vec::with_capacity(self.stts.len());
        for (ref lang, stt) in self.stts.iter_mut() {
            results.push((lang, confidence_for(stt, audio).await?));
        }

        results.sort_by(|(_,c1),(_,c2)| (c1).partial_cmp(c2).unwrap_or(Ordering::Equal));

        Ok(results[0].0.to_owned())
    }
}