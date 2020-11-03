use std::sync::PoisonError/*To make a sendable error*/;
use thiserror::Error;
#[derive(Error, Debug)]
#[error("Poisoned lock: an error happened inside")]
pub struct SendablePoisonError;

impl<A> From<PoisonError<A>> for SendablePoisonError {
    fn from(_other: PoisonError<A>) -> SendablePoisonError {
        Self
    }
}

pub trait MakeSendable<A> {
    fn sendable(self) -> Result<A,SendablePoisonError>;
}

impl<A,B> MakeSendable<A> for Result<A,PoisonError<B>> {
    fn sendable(self) -> Result<A,SendablePoisonError> {
        self.map_err(|e|e.into())
    }
}