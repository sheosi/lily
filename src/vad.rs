use std::path::Path;


pub trait Vad {
	fn reset(&mut self);
	fn is_someone_talking(&mut self, audio: &[i16]) -> bool;
}


pub struct SnowboyVad {
	vad: rsnowboy::SnowboyVad,
}

impl SnowboyVad {
	pub fn new(res_path: &Path) -> Self {
		let vad = rsnowboy::SnowboyVad::new(res_path.to_str().unwrap());

		Self {vad}
	}
}

impl Vad for SnowboyVad {
	fn reset(&mut self) {
		self.vad.reset();
	}

	fn is_someone_talking(&mut self, audio: &[i16]) -> bool {
		let vad_val = self.vad.run_short_array(&audio[0] as *const i16, audio.len() as i32, false);
		if vad_val == -1 { // Maybe whe should do something worse with this is (return a result)
			log::error!("Something happened in the Vad");
		}

		vad_val == 0
	}
}

