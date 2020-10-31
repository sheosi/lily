pub mod audio;
pub mod communication;
pub mod hotword;
pub mod vad;
pub mod vars;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
