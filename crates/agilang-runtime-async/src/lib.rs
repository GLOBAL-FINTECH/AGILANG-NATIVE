//! Native asynchronous executor, cancellation, bounded channels, timers, and graceful shutdown.

use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use once_cell::sync::OnceCell;
use tokio::{
    runtime::{Builder, Runtime},
    sync::mpsc,
    task::JoinHandle,
};

static GLOBAL_RUNTIME: OnceCell<Arc<Runtime>> = OnceCell::new();

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub async fn cancelled(&self) {
        while !self.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

pub struct BoundedSender<T>(mpsc::Sender<T>);
pub struct BoundedReceiver<T>(mpsc::Receiver<T>);

pub fn bounded_channel<T>(
    capacity: usize,
) -> RuntimeResult<(BoundedSender<T>, BoundedReceiver<T>)> {
    if capacity == 0 {
        return Err(AgilangError::invalid_argument(
            "channel capacity must be greater than zero",
        ));
    }
    let (tx, rx) = mpsc::channel(capacity);
    Ok((BoundedSender(tx), BoundedReceiver(rx)))
}

impl<T> Clone for BoundedSender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> BoundedSender<T> {
    pub async fn send(&self, value: T) -> RuntimeResult<()> {
        self.0
            .send(value)
            .await
            .map_err(|_| AgilangError::new(ErrorCode::Cancelled, "channel receiver was closed"))
    }
}
impl<T> BoundedReceiver<T> {
    pub async fn receive(&mut self) -> Option<T> {
        self.0.recv().await
    }
}

pub fn global_runtime() -> RuntimeResult<&'static Arc<Runtime>> {
    GLOBAL_RUNTIME.get_or_try_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .thread_name("agilang-worker")
            .thread_stack_size(2 * 1024 * 1024)
            .build()
            .map(Arc::new)
            .map_err(AgilangError::from)
    })
}

pub fn spawn<F, T>(future: F) -> RuntimeResult<JoinHandle<T>>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    Ok(global_runtime()?.spawn(future))
}

pub fn block_on<F: Future>(future: F) -> RuntimeResult<F::Output> {
    Ok(global_runtime()?.block_on(future))
}
pub async fn sleep(milliseconds: u64) {
    tokio::time::sleep(Duration::from_millis(milliseconds)).await;
}
pub async fn timeout<F: Future>(milliseconds: u64, future: F) -> RuntimeResult<F::Output> {
    tokio::time::timeout(Duration::from_millis(milliseconds), future)
        .await
        .map_err(|_| AgilangError::new(ErrorCode::Timeout, "asynchronous operation timed out"))
}
pub async fn cancellable<F: Future>(
    token: &CancellationToken,
    future: F,
) -> RuntimeResult<F::Output> {
    tokio::select! {
        value = future => Ok(value),
        _ = token.cancelled() => Err(AgilangError::new(ErrorCode::Cancelled, "operation was cancelled")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_and_cancellation_work() {
        block_on(async {
            let (tx, mut rx) = bounded_channel(2).unwrap();
            tx.send(42).await.unwrap();
            assert_eq!(rx.receive().await, Some(42));
            let token = CancellationToken::new();
            token.cancel();
            assert!(cancellable(&token, sleep(100)).await.is_err());
        })
        .unwrap();
    }
}
