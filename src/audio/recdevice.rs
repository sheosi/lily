use std::time::{SystemTime, Duration, UNIX_EPOCH};
use crate::vars::{CLOCK_TOO_EARLY_MSG, DEFAULT_SAMPLES_PER_SECOND, RECORD_BUFFER_SIZE};
use log::info;

#[cfg(feature = "devel_cpal_rec")]
use cpal::traits::{DeviceTrait, HostTrait, EventLoopTrait};
#[cfg(feature = "devel_cpal_rec")]
use cpal::{SampleFormat, StreamId};
#[cfg(feature = "devel_cpal_rec")]
use ringbuf::{Consumer, RingBuffer};

pub trait Recording {
    fn read(&mut self) -> Result<Option<&[i16]>, std::io::Error>;
    fn read_for_ms(&mut self, milis: u16) -> Result<Option<&[i16]>, std::io::Error>;
    fn start_recording(&mut self) -> Result<(), std::io::Error>;
    fn stop_recording(&mut self) -> Result<(), std::io::Error>;
}

#[cfg(not(feature = "devel_cpal_rec"))]
pub struct RecDevice {
    device: sphinxad::AudioDevice,
    buffer: [i16; RECORD_BUFFER_SIZE],
    last_read: u128
}

#[cfg(not(feature = "devel_cpal_rec"))]
impl RecDevice {
    pub fn new() -> Result<RecDevice, std::io::Error> {
        info!("Using sphinxad");
        //let host = cpal::default_host();
        //let device = host.default_input_device().expect("Something failed");

        let device = sphinxad::AudioDevice::default_with_sps(DEFAULT_SAMPLES_PER_SECOND as usize)?;

        Ok(RecDevice {
            device,
            buffer: [0i16; RECORD_BUFFER_SIZE],
            last_read: 0
        })

    }

    fn get_millis() -> u128 {
        SystemTime::now().duration_since(UNIX_EPOCH).expect(CLOCK_TOO_EARLY_MSG).as_millis()
    }
}

#[cfg(not(feature = "devel_cpal_rec"))]
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

// Cpal version
#[cfg(feature = "devel_cpal_rec")]
pub struct RecDevice {
    external_buffer: [i16; RECORD_BUFFER_SIZE],
    internal_buffer_consumer: Consumer<i16>,
    last_read: u128,
    stream_id: StreamId
}

#[cfg(feature = "devel_cpal_rec")]
impl RecDevice {
    // For now just use that error to original RecDevice
    pub fn new() -> Result<Self, std::io::Error> {
        info!("Using cpal");
        let host = cpal::default_host();
        let device = host.default_input_device().unwrap();
        let mut format = device.default_input_format().unwrap();
        format.data_type = SampleFormat::I16;
        format.channels = 1;
        format.sample_rate = cpal::SampleRate(DEFAULT_SAMPLES_PER_SECOND);
        let event_loop = host.event_loop();
        let stream_id = event_loop.build_input_stream(&device, &format).unwrap();
        event_loop.play_stream(stream_id.clone()).unwrap();

        let internal_buffer = RingBuffer::new(RECORD_BUFFER_SIZE);
        let (mut prod, cons) = internal_buffer.split();

        std::thread::spawn(move || {
            event_loop.run(move |id, event|{
                let data = match event {
                    Ok(data) => data,
                    Err(err) => {
                        eprintln!("An error ocurred on stream {:?}: {}", id, err);
                        return;
                    }
                };

                //Otherwise send data
                match data {
                    cpal::StreamData::Input {buffer: cpal::UnknownTypeInputBuffer::U16(buffer)} => {
                        println!("Note: Recording using U16 translation code, has not been tested and may not work");
                        let mut count = 0;
                        prod.push_each(||{
                            let res = if count < buffer.len() {
                                Some(((buffer[count] as u32) - ((std::u16::MAX/2) as u32)) as i16)
                            }
                            else {
                                None
                            };

                            count += 1;
                            res
                        });
                    }
                    cpal::StreamData::Input {buffer: cpal::UnknownTypeInputBuffer::I16(buffer)} => {
                        prod.push_slice(&buffer);
                    }
                    cpal::StreamData::Input {buffer: cpal::UnknownTypeInputBuffer::F32(buffer)} => {
                        println!("Note: Recording using F32 translation code, has not been tested and may not work");
                        let mut count = 0;
                        prod.push_each(||{
                            let res = if count < buffer.len() {
                                Some((buffer[count]  * (std::i16::MAX as f32)) as i16)
                            }
                            else {
                                None
                            };

                            count += 1;
                            res
                        });
                    }
                    _ => {
                        panic!("Received non-input data");
                    }
                }
            })
        });

        Ok(RecDevice {
            external_buffer: [0i16; RECORD_BUFFER_SIZE],
            last_read: 0,
            internal_buffer_consumer: cons,
            stream_id
        })

    }

    fn get_millis() -> u128 {
        SystemTime::now().duration_since(UNIX_EPOCH).expect(CLOCK_TOO_EARLY_MSG).as_millis()
    }
}

#[cfg(feature = "devel_cpal_rec")]
impl Recording for RecDevice {
    fn read(&mut self) -> Result<Option<&[i16]>, std::io::Error> {
        self.last_read = Self::get_millis();
        let size = self.internal_buffer_consumer.pop_slice(&mut self.external_buffer[..]);
        if size > 0 {
            Ok(Some(&self.external_buffer[0..size]))
        }
        else {
            Ok(None)
        }
        
    }
    fn read_for_ms(&mut self, milis: u16) -> Result<Option<&[i16]>, std::io::Error> {
        let curr_time = Self::get_millis();
        let diff_time = (curr_time - self.last_read) as u16;
        if milis > diff_time{
            let sleep_time = (milis  - diff_time) as u64 ;
            std::thread::sleep(Duration::from_millis(sleep_time));
        }

        self.read()
    }

    fn start_recording(&mut self) -> Result<(), std::io::Error> {
        let host = cpal::default_host();
        let el = host.event_loop();
        el.play_stream(self.stream_id.clone()).unwrap();
        Ok(())
    }
    fn stop_recording(&mut self) -> Result<(), std::io::Error> {
        let host = cpal::default_host();
        let el = host.event_loop();
        el.pause_stream(self.stream_id.clone()).unwrap();
        Ok(())
    }
}
