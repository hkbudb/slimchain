#[macro_use]
extern crate log;

use crossbeam::{
    deque::{Injector, Stealer, Worker},
    queue::{ArrayQueue, SegQueue},
    sync::{Parker, Unparker},
    utils::Backoff,
};
use slimchain_common::{
    basic::{BlockHeight, H256},
    create_id_type_u32,
    error::Result,
    tx::TxTrait,
    tx_req::SignedTxRequest,
};
use slimchain_tx_state::{TxStateView, TxWriteSetPartialTrie};
use std::{
    iter,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

create_id_type_u32!(TxTaskId);

pub trait TxEngineWorker: Send {
    type Output: TxTrait;

    fn execute(&self, task: TxTask) -> Result<Self::Output>;
}

pub struct TxTask {
    pub id: TxTaskId,
    pub block_height: BlockHeight,
    pub state_view: Arc<dyn TxStateView + Sync + Send>,
    pub state_root: H256,
    pub signed_tx_req: SignedTxRequest,
}

impl TxTask {
    pub fn new(
        block_height: BlockHeight,
        state_view: Arc<dyn TxStateView + Sync + Send>,
        state_root: H256,
        signed_tx_req: SignedTxRequest,
    ) -> Self {
        let id = TxTaskId::next_id();

        Self {
            id,
            block_height,
            state_view,
            state_root,
            signed_tx_req,
        }
    }

    pub fn get_id(&self) -> TxTaskId {
        self.id
    }
}

pub struct TxTaskOutput<TxOutput: TxTrait> {
    pub task_id: TxTaskId,
    pub tx_output: TxOutput,
    pub write_trie: TxWriteSetPartialTrie,
    pub exec_time: Duration,
}

pub struct TxEngine<TxOutput: TxTrait + 'static> {
    task_queue: Arc<Injector<TxTask>>,
    result_queue: Arc<SegQueue<TxTaskOutput<TxOutput>>>,
    unparker_queue: Arc<ArrayQueue<Unparker>>,
    shutdown_flag: Arc<AtomicBool>,
    worker_threads: Vec<JoinHandle<()>>,
}

impl<TxOutput: TxTrait + 'static> TxEngine<TxOutput> {
    pub fn new(
        threads: usize,
        worker_factory: impl Fn() -> Box<dyn TxEngineWorker<Output = TxOutput>>,
    ) -> Self {
        info!("Spawning TxEngine workers in {} threads.", threads);

        let task_queue = Arc::new(Injector::new());
        let result_queue = Arc::new(SegQueue::new());
        let unparker_queue = Arc::new(ArrayQueue::new(threads));
        let shutdown_flag = Arc::new(AtomicBool::new(false));

        let mut workers: Vec<_> = (0..threads)
            .map(|_| {
                TxEngineWorkerInstance::new(
                    worker_factory(),
                    task_queue.clone(),
                    threads - 1,
                    result_queue.clone(),
                    unparker_queue.clone(),
                    shutdown_flag.clone(),
                )
            })
            .collect();

        let stealers: Vec<_> = workers.iter().map(|w| w.get_local_stealer()).collect();

        for (i, worker) in workers.iter_mut().enumerate() {
            for (j, stealer) in stealers.iter().enumerate() {
                if i != j {
                    worker.add_global_stealer(stealer.clone());
                }
            }
        }

        let worker_threads: Vec<_> = workers
            .into_iter()
            .map(|w| thread::spawn(move || w.run()))
            .collect();

        Self {
            task_queue,
            result_queue,
            unparker_queue,
            shutdown_flag,
            worker_threads,
        }
    }

    pub fn push_task(&self, task: TxTask) {
        self.task_queue.push(task);
        if let Ok(unpaker) = self.unparker_queue.pop() {
            unpaker.unpark();
        }
    }

    pub fn pop_result(&self) -> Option<TxTaskOutput<TxOutput>> {
        self.result_queue.pop().ok()
    }

    pub fn pop_or_wait_result(&self) -> TxTaskOutput<TxOutput> {
        let backoff = Backoff::new();
        loop {
            if let Some(output) = self.pop_result() {
                return output;
            }

            backoff.snooze();
        }
    }
}

impl<TxOutput: TxTrait + 'static> Drop for TxEngine<TxOutput> {
    fn drop(&mut self) {
        self.shutdown_flag.store(true, Ordering::Release);

        while let Ok(unpacker) = self.unparker_queue.pop() {
            unpacker.unpark();
        }

        info!("Waiting TxEngine workers to be shutdown.");
        for w in self.worker_threads.drain(..) {
            w.join()
                .expect("TxEngine: Failed to join the worker thread.");
        }
        info!("TxEngine is shutdown.");
    }
}

struct TxEngineWorkerInstance<TxOutput: TxTrait> {
    global_task_queue: Arc<Injector<TxTask>>,
    local_task_queue: Worker<TxTask>,
    stealers: Vec<Stealer<TxTask>>,
    result_queue: Arc<SegQueue<TxTaskOutput<TxOutput>>>,
    unparker_queue: Arc<ArrayQueue<Unparker>>,
    shutdown_flag: Arc<AtomicBool>,
    worker: Box<dyn TxEngineWorker<Output = TxOutput>>,
}

impl<TxOutput: TxTrait> TxEngineWorkerInstance<TxOutput> {
    fn new(
        worker: Box<dyn TxEngineWorker<Output = TxOutput>>,
        global_task_queue: Arc<Injector<TxTask>>,
        stealer_num: usize,
        result_queue: Arc<SegQueue<TxTaskOutput<TxOutput>>>,
        unparker_queue: Arc<ArrayQueue<Unparker>>,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Self {
        let local_task_queue = Worker::new_fifo();

        Self {
            global_task_queue,
            local_task_queue,
            stealers: Vec::with_capacity(stealer_num),
            result_queue,
            unparker_queue,
            shutdown_flag,
            worker,
        }
    }

    fn get_local_stealer(&self) -> Stealer<TxTask> {
        self.local_task_queue.stealer()
    }

    fn add_global_stealer(&mut self, stealer: Stealer<TxTask>) {
        self.stealers.push(stealer);
    }

    fn find_task(&self) -> Option<TxTask> {
        self.local_task_queue.pop().or_else(|| {
            iter::repeat_with(|| {
                self.global_task_queue
                    .steal_batch_and_pop(&self.local_task_queue)
                    .or_else(|| self.stealers.iter().map(|s| s.steal()).collect())
            })
            .find(|s| !s.is_retry())
            .and_then(|s| s.success())
        })
    }

    fn wait_until_task(&self) -> Option<TxTask> {
        let backoff = Backoff::new();
        loop {
            match self.find_task() {
                Some(task) => return Some(task),
                None => {
                    if backoff.is_completed() {
                        if self.shutdown_flag.load(Ordering::Acquire) {
                            return None;
                        }

                        let parker = Parker::new();
                        self.unparker_queue
                            .push(parker.unparker().clone())
                            .expect("TxEngine: Failed to send unpaker.");
                        parker.park();
                    } else {
                        backoff.snooze();
                    }
                }
            }
        }
    }

    fn run(&self) {
        loop {
            let task = match self.wait_until_task() {
                Some(task) => task,
                None => break,
            };
            let begin = Instant::now();
            let task_id = task.get_id();
            let state_view = task.state_view.clone();
            let root_address = task.state_root;
            let tx_output = match self.worker.execute(task) {
                Ok(output) => output,
                Err(e) => {
                    error!("TxEngine: Failed to execute task. Error: {}", e);
                    continue;
                }
            };
            let write_trie =
                match TxWriteSetPartialTrie::new(state_view, root_address, tx_output.tx_writes()) {
                    Ok(trie) => trie,
                    Err(e) => {
                        error!(
                            "TxEngine: Failed to create TxWriteSetPartialTrie. Error: {}",
                            e
                        );
                        continue;
                    }
                };
            self.result_queue.push(TxTaskOutput {
                task_id,
                tx_output,
                write_trie,
                exec_time: Instant::now() - begin,
            });
        }
    }
}
