use std::path::Path;
use log::{debug, info};

pub trait HotwordDetector {
    fn start_hotword_check(&mut self);
    fn check_hotword(&mut self, audio: &[i16]) -> bool;
}

pub struct Snowboy {
    vad: rsnowboy::SnowboyVad,
    detector: rsnowboy::SnowboyDetect,
    someone_talking: bool
}


impl Snowboy {
    pub fn new(model_path: &Path, res_path: &Path) -> Snowboy {

        let vad = rsnowboy::SnowboyVad::new(res_path.to_str().unwrap());

        let detector = rsnowboy::SnowboyDetect::new(res_path.to_str().unwrap(), model_path.to_str().unwrap());
        detector.set_sensitivity("0.45");
        detector.set_audio_gain(1.0);
        detector.apply_frontend(false);

        Snowboy {vad, detector, someone_talking: false}
    }

    pub fn detector_check(&mut self, audio: &[i16]) -> i32 {
        self.detector.run_short_array_detection(&audio[0] as *const i16, audio.len() as i32, false)
    }
}

impl HotwordDetector for Snowboy {
    fn start_hotword_check(&mut self) {
        self.detector.reset();
        self.vad.reset();
        self.someone_talking = false;
        info!("WaitingForHotword");
    }

    fn check_hotword(&mut self, audio: &[i16]) -> bool {
            if !self.someone_talking {
                let vad_val = self.vad.run_short_array(&audio[0] as *const i16, audio.len() as i32, false);
                /*match vad_val {
                    -2 => {println!("Silence");}
                    -1 => {println!("Wait something happened");}
                    0 => {println!("Something is there");}
                    _ => {}

                }*/

                let vad_res = vad_val == 0;


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
                self.someone_talking = false;
            } 
            detector_res == 1
        }
    }
}