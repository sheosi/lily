use std::path::Path;
use crate::vad::Vad;
use log::{debug, info};

pub trait HotwordDetector {
    fn start_hotword_check(&mut self);
    fn check_hotword(&mut self, audio: &[i16]) -> bool;
}

pub struct Snowboy {
    vad: crate::vad::SnowboyVad,
    detector: rsnowboy::SnowboyDetect,
    someone_talking: bool
}


impl Snowboy {
    pub fn new(model_path: &Path, res_path: &Path) -> Snowboy {

        let vad = crate::vad::SnowboyVad::new(res_path);

        let detector = rsnowboy::SnowboyDetect::new(res_path.to_str().unwrap(), model_path.to_str().unwrap());
        detector.set_sensitivity("0.50");
        detector.set_audio_gain(1.0);
        detector.apply_frontend(false);

        Snowboy {vad, detector, someone_talking: true}
    }

    pub fn detector_check(&mut self, audio: &[i16]) -> i32 {
        self.detector.run_short_array_detection(&audio[0] as *const i16, audio.len() as i32, false)
    }
}

impl HotwordDetector for Snowboy {
    fn start_hotword_check(&mut self) {
        self.detector.reset();
        self.vad.reset();
        //self.someone_talking = false;
        info!("WaitingForHotword");
    }

    fn check_hotword(&mut self, audio: &[i16]) -> bool {
        if !self.someone_talking {
            let vad_res = self.vad.is_someone_talking(audio);
            /*match vad_val {
                -2 => {println!("Silence");}
                -1 => {println!("Wait something happened");}
                0 => {println!("Something is there");}
                _ => {}

            }*/


            if vad_res == true {
                debug!("I can hear someone");
                self.someone_talking = true;
                let detector_res = self.detector_check(audio);
                if detector_res == -2 {
                    debug!("You stopped talking");
                    self.someone_talking = false;
                } 
                detector_res == 1
            }
            else {
                false
            }
        }
        else {
            let detector_res = self.detector_check(audio);
            if detector_res == -2 {
                //self.someone_talking = false;
            } 
            detector_res == 1
        }
    }
}