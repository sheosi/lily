#[derive(Debug, Clone, Copy)]
pub enum Lang {
    EsEs,
    EnUs
}

impl Lang {
	pub fn iso_str(self) -> &'static str {
		match self {
			Lang::EsEs => {"es-ES"}
			Lang::EnUs => {"en-US"}
		}
		
	}
}






