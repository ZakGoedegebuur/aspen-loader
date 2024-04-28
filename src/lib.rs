use std::{
    collections::VecDeque, 
    num::NonZeroUsize, 
    sync::{
        atomic::{
            AtomicBool, 
            Ordering
        }, 
        mpsc, 
        Arc, 
        Condvar, 
        Mutex
    }, 
    thread::{
        self, 
        JoinHandle
    }
};

use subloader::{
    LoadExp, 
    SubLoader
};

mod subloader;

pub trait Loadable {
    type Output: Send + Sync;
    fn load(&mut self) -> Self::Output;
}

type LoadQueue = Arc<Mutex<VecDeque<Box<dyn LoadExp + Send + Sync>>>>;
pub struct Loader {
    threads: Vec<Option<thread::JoinHandle<()>>>,
    queue: LoadQueue,
    conds: Arc<(Condvar, Mutex<bool>, AtomicBool)>,
}

impl Loader {
    pub fn new() -> Self {
        let mut threads = Vec::new();

        let num_threads = std::thread::available_parallelism()
            .unwrap_or(NonZeroUsize::new(4).unwrap())
            .get();

        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let conds = Arc::new((Condvar::new(), Mutex::new(false), AtomicBool::new(false)));

        for _ in 0..num_threads {
            let queue = queue.clone();
            let conds = conds.clone();
            threads.push(Some(thread::spawn(|| { Self::loader_func(queue, conds) })))
        }

        Self {
            threads,
            queue,
            conds,
        }
    }

    pub fn load<T: Loadable + Send + Sync + 'static>(&mut self, item: T) -> mpsc::Receiver<<T as Loadable>::Output> {
        let (loader, reciever) = SubLoader::new(item);

        self.queue
            .lock()
            .unwrap()
            .push_back(loader);

        self.conds.0.notify_one();

        reciever
    }

    fn loader_func(queue: LoadQueue, conditions: Arc<(Condvar, Mutex<bool>, AtomicBool)>) {
        let (ref cond_var, ref mutex, ref should_close) = *conditions;

        while !should_close.load(Ordering::SeqCst) {
            {
                let len = {
                    queue.lock().unwrap().len()
                };

                if len < 1 {
                    let guard = mutex.lock().unwrap();
                    let _x = cond_var.wait(guard).unwrap();
                }
            }
            
            let data = {
                let mut queue = queue.lock().unwrap();
                queue.pop_front()
            };
            
            match data {
                Some(mut data) => {
                    data.load();
                    
                    continue
                },
                None => {
                }
            }
        }
    }

    /// Checks each thread to see if it has returned, and if it has returned, replaces the thread
    pub fn restore_threads(&mut self) {
        for handle in self.threads.iter_mut() {
            let old = handle.take();

            fn add_threads(
                queue: LoadQueue, 
                conds: Arc<(Condvar, Mutex<bool>, AtomicBool)>, 
                handle: &mut Option<JoinHandle<()>>
            ) {
                *handle = Some(thread::spawn(|| { Loader::loader_func(queue, conds) }));
            }

            match old {
                Some(thread) => {
                    // Check if thread has closed
                    if thread.is_finished() {
                        _ = thread.join();

                        add_threads(self.queue.clone(), self.conds.clone(), handle)
                    }
                },
                None => {
                    add_threads(self.queue.clone(), self.conds.clone(), handle)
                }
            }
        }
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        let (ref cond_var, _, ref should_close) = *self.conds; 

        should_close.store(true, Ordering::SeqCst);
        cond_var.notify_all();

        for opt in self.threads.iter_mut() {
            let thread = opt.take();

            match thread {
                Some(thread) => {
                    _ = thread.join()
                },
                None => () 
            }
        }
    }
}