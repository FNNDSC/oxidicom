use either::Either;
use tokio::sync::mpsc::error::SendError;

pub(crate) fn send_error_left<L, R>(error: SendError<L>) -> SendError<Either<L, R>> {
    SendError(Either::Left(error.0))
}

pub(crate) fn send_error_right<L, R>(error: SendError<R>) -> SendError<Either<L, R>> {
    SendError(Either::Right(error.0))
}
