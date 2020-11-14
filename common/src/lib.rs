pub mod audio;
pub mod communication;
pub mod extensions;
#[cfg(feature = "client")]
pub mod hotword;
pub mod other;
#[cfg(feature = "client")]
pub mod vad;
pub mod vars;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
