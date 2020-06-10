use std::time::{SystemTime, Duration, UNIX_EPOCH};
use crate::vars::CLOCK_TOO_EARLY_MSG;

#[cfg(feature = "devel_cpal_rec")]
use cpal::traits::HostTrait;

pub struct RecDevice {
    device: sphinxad::AudioDevice,
    buffer: [i16; 4096],
    last_read: u128
}

pub trait Recording {
    fn read(&mut self) -> Result<Option<&[i16]>, std::io::Error>;
    fn read_for_ms(&mut self, milis: u16) -> Result<Option<&[i16]>, std::io::Error>;
    fn start_recording(&mut self) -> Result<(), std::io::Error>;
    fn stop_recording(&mut self) -> Result<(), std::io::Error>;
}

impl RecDevice {
    pub fn new() -> Result<RecDevice, std::io::Error> {
        //let host = cpal::default_host();
        //let device = host.default_input_device().expect("Something failed");

        let device = sphinxad::AudioDevice::default_with_sps(16000)?;

        Ok(RecDevice {
            device,
            buffer: [0i16; 4096],
            last_read: 0
        })

    }

    fn get_millis() -> u128 {
        SystemTime::now().duration_since(UNIX_EPOCH).expect(CLOCK_TOO_EARLY_MSG).as_millis()
    }
}

impl Recording for RecDevice {
    fn read(&mut self) -> Result<Option<&[i16]>, std::io::Error> {
        self.last_read = Self::get_millis();
        self.device.read(&mut self.buffer[..])
    }

    fn read_for_ms(&mut self, milis: u16) -> Result<Option<&[i16]>, std::io::Error> {
        let curr_time = Self::get_millis();
        let diff_time = (curr_time - self.last_read) as u16;
        if milis > diff_time{
            let sleep_time = (milis  - diff_time) as u64 ;
            std::thread::sleep(Duration::from_millis(sleep_time));
        }
        else {
            //log::info!("We took {}ms more from what we were asked ({})", diff_time - milis, milis);
        }
        
        self.read()
    }

    fn start_recording(&mut self) -> Result<(), std::io::Error> {
        self.last_read = SystemTime::now().duration_since(UNIX_EPOCH).expect(CLOCK_TOO_EARLY_MSG).as_millis();
        self.device.start_recording()
    }
    fn stop_recording(&mut self) -> Result<(), std::io::Error> {
        self.device.stop_recording()
    }
}

#[cfg(feature = "devel_cpal_rec")]
pub struct RecDeviceCpal {
    device: cpal::Device,
    buffer: [i16; 2048],
}

#[cfg(feature = "devel_cpal_rec")]
impl RecDeviceCpal {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let device = host.default_input_device().expect("Something failed");
        //let format = 

        RecDeviceCpal {
            device,
            buffer: [0i16; 2048],
        }

    }
}

#[cfg(feature = "devel_cpal_rec")]
impl Recording for RecDeviceCpal {
    fn read(&mut self) -> Option<&[i16]> {
        None
        // NYI
        //
        //self.device.read(&mut self.buffer[..])
    }
    fn read_for_ms(&mut self, milis: u16) -> Option<&[i16]> {
        None
    }

    fn start_recording(&mut self) -> Result<(), std::io::Error> {
        //self.device.start_recording()   
        // NYI
        Ok(())
    }
    fn stop_recording(&mut self) -> Result<(), std::io::Error> {
        //self.device.stop_recording()
        // NYI
        Ok(())
    }
}
