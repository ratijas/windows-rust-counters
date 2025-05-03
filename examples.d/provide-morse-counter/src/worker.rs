use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread::JoinHandle;

pub struct WorkerThread<T> {
    thread: JoinHandle<T>,
    cancellation_token: Arc<AtomicBool>,
}

impl<T> WorkerThread<T>
where
    T: Send + 'static,
{
    /// Spawn new worker with new cancellation token.
    pub fn spawn(f: impl FnOnce(Arc<AtomicBool>) -> T + Send + 'static) -> Self {
        let cancellation_token = Arc::new(AtomicBool::new(false));
        let token_clone = Arc::clone(&cancellation_token);
        let thread = std::thread::spawn(move || f(token_clone));
        WorkerThread {
            thread,
            cancellation_token,
        }
    }

    /// Cancel the worker by setting cancellation token to true.
    pub fn cancel(&self) {
        self.cancellation_token
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Cancel the worker and blocking wait for it to finish.
    pub fn join(self) -> std::thread::Result<T> {
        self.cancel();
        self.thread.join()
    }
}
