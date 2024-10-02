//! Thread pool implementation from The Book.
//! <https://doc.rust-lang.org/book/ch20-02-multithreaded.html>

use std::sync::{mpsc, Arc, Mutex};
use std::thread;

type Job = Box<dyn FnOnce() + Send + 'static>;

/// Simple thread pool
pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>,
}

impl ThreadPool {
    /// Create a thread pool
    pub fn new(size: usize, name: &'static str) -> ThreadPool {
        if size == 0 {
            panic!("Thread pool cannot have 0 threads.")
        }

        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let workers = (0..size)
            .map(|id| Worker::new(id, Arc::clone(&receiver), name))
            .collect();

        ThreadPool {
            workers,
            sender: Some(sender),
        }
    }

    /// Run a job on in this thread pool.
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);
        self.sender
            .as_ref()
            .expect("thread pool has been shut down")
            .send(job)
            .unwrap();
    }

    /// Close the thread pool.
    ///
    /// Note: unlike The Book, the cleanup code is here as a method instead of the Drop trait
    /// so that CTRL-C aborts threads immediately instead of waiting for them to finish.
    pub fn shutdown(&mut self) {
        drop(self.sender.take());
        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}

struct Worker {
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(
        id: usize,
        receiver: Arc<Mutex<mpsc::Receiver<Job>>>,
        pool_name: &'static str,
    ) -> Worker {
        let thread = thread::spawn(move || {
            tracing::info!("Starting worker {pool_name}/{id}");
            loop {
                let message = receiver.lock().unwrap().recv();
                match message {
                    Ok(job) => {
                        job();
                    }
                    Err(_) => {
                        tracing::info!("Shutting down worker {pool_name}/{id}");
                        break;
                    }
                }
            }
        });

        Worker {
            thread: Some(thread),
        }
    }
}
