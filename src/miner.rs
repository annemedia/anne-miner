use crate::com::api::MiningInfoResponse as MiningInfo;
use crate::config::{Cfg, MiningMode}; 
use crate::cpu_worker::create_cpu_worker_task;
use crate::future::interval::Interval;
use std::cell::Cell;
#[cfg(feature = "opencl")]
use crate::gpu_worker::create_gpu_worker_task;
#[cfg(feature = "opencl")]
use crate::gpu_worker_async::create_gpu_worker_task_async;
#[cfg(feature = "opencl")]
use crate::ocl::GpuBuffer;
#[cfg(feature = "opencl")]
use crate::ocl::GpuContext;
use crate::plot::{Plot, SCOOP_SIZE};
use crate::poc_hashing;
use crate::reader::Reader;
use crate::requests::RequestHandler;
use crate::utils::{get_device_id, new_thread_pool};
use crossbeam_channel;
use filetime::FileTime;
use futures_util::{stream::StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
#[cfg(feature = "opencl")]
use ocl_core::Mem;
use std::cmp::{max, min};
use std::collections::HashMap;
use std::fs::read_dir;
use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex};
use std::thread;
use std::u64;
use stopwatch::Stopwatch;
use tokio::runtime::Handle;
use std::alloc::*;
use std::sync::atomic::{AtomicBool, Ordering};
use page_size;
use std::collections::HashSet;

pub static SUBMISSION_FAILED: AtomicBool = AtomicBool::new(false);
pub static SUBMISSION_SUCCESS: AtomicBool = AtomicBool::new(false);
pub static SHOULD_STOP_MINING: AtomicBool = AtomicBool::new(false);

pub struct Miner {
    reader: Reader,
    request_handler: RequestHandler,
    rx_nonce_data: mpsc::Receiver<NonceData>,
    target_deadline: u64,
    account_id_to_target_deadline: HashMap<u64, u64>,
    state: Arc<Mutex<State>>,
    reader_task_count: usize,
    get_mining_info_interval: u64,
    executor: Handle,
    wakeup_after: i64,
    submit_only_best: bool,
    mining_mode: MiningMode,
}

pub struct State {
    pub generation_signature: String,
    pub generation_signature_bytes: [u8; 32],
    pub height: u64,
    pub block: u64,
    pub account_id_to_best_deadline: HashMap<u64, u64>,
    pub annode_target_deadline: u64,
    pub sw: Stopwatch,
    pub scanning: bool,
    pub processed_reader_tasks: usize,
    pub scoop: u32,
    pub first: bool,
    pub outage: bool,
    pub share_mining_enabled: bool, 
    pub solo_mining_enabled: bool, 
    pub mining_mode: MiningMode, 
    pub submitted_for_this_block: bool,
    pub finished_scanning: bool,
    pub last_progress_log: f64,
}

impl State {
    fn new() -> Self {
        Self {
            generation_signature: "".to_owned(),
            height: 0,
            block: 0,
            scoop: 0,
            account_id_to_best_deadline: HashMap::new(),
            annode_target_deadline: u64::MAX,
            processed_reader_tasks: 0,
            sw: Stopwatch::new(),
            generation_signature_bytes: [0; 32],
            scanning: false,
            first: true,
            outage: false,
            share_mining_enabled: false, 
            solo_mining_enabled: false, 
            mining_mode: MiningMode::Solo, 
            submitted_for_this_block: false,
            finished_scanning: false,
            last_progress_log: 0.0,
        }
    }


    #[allow(unused_variables)]
    fn update_mining_info(&mut self, mining_info: &MiningInfo, _miner_mode: MiningMode) {
        thread_local! {
            static MESSAGE_COUNTER2: Cell<u32> = Cell::new(0);
        }

        self.mining_mode = _miner_mode;
        SHOULD_STOP_MINING.store(mining_info.amp != "MINING", Ordering::Relaxed);

        let is_new_block = mining_info.height != self.height;
        
        if is_new_block {
            self.submitted_for_this_block = false;
            self.finished_scanning = false;
            self.processed_reader_tasks = 0;
            self.last_progress_log = 0.0;
   
            for best_deadlines in self.account_id_to_best_deadline.values_mut() {
                *best_deadlines = u64::MAX;
            }
            self.block += 1;
            
            self.generation_signature_bytes = poc_hashing::decode_gensig(&mining_info.generation_signature);
            self.generation_signature = mining_info.generation_signature.clone();
        }
        
        self.height = mining_info.height;
        self.annode_target_deadline = mining_info.target_deadline;
        
        self.solo_mining_enabled = mining_info.annode_mode == "LIVE" && 
                                mining_info.amp == "MINING";
        
        self.share_mining_enabled = mining_info.annode_mode == "LIVE" && 
                                mining_info.amp == "MINING" && 
                                mining_info.share_mining_ok;

        trace!(
            "GUI_JSON {{ \"type\": \"mining_status\", \"height\": {}, \"annode_mode\": \"{}\", \"amp\": \"{}\", \"share_mining_ok\": {}, \"solo_enabled\": {}, \"share_enabled\": {}, \"mining_mode\": \"{}\" }}",
            self.height,
            mining_info.annode_mode,
            mining_info.amp,
            mining_info.share_mining_ok,
            self.solo_mining_enabled,
            self.share_mining_enabled,
            match _miner_mode {
                MiningMode::Solo => "SOLO",
                MiningMode::Share => "SHARE",
            }
        );

        if is_new_block {
            if self.solo_mining_enabled && self.share_mining_enabled {
                info!(
                    "{: <80}",
                    format!("new block: height={},**{}**", 
                            mining_info.height, "SOLO + SHARE mining round")
                );

                trace!(
                    "GUI_JSON {{ \"type\": \"round_info\", \"height\": {}, \"round_type\": \"SOLO + SHARE\", \"block\": {} }}",
                    self.height, self.block
                );
            } 
            else if self.solo_mining_enabled {
                info!(
                    "{: <80}",
                    format!("new block: height={}, **{}**", 
                            mining_info.height, "SOLO mining round only")
                );

                trace!(
                    "GUI_JSON {{ \"type\": \"round_info\", \"height\": {}, \"round_type\": \"SOLO ONLY\", \"block\": {} }}",
                    self.height, self.block
                );
            }
            else {

                trace!(
                    "GUI_JSON {{ \"type\": \"round_info\", \"height\": {}, \"round_type\": \"INACTIVE\", \"block\": {} }}",
                    self.height, self.block
                );
            }
        }


        if mining_info.annode_mode != "LIVE" {             
            MESSAGE_COUNTER2.with(|counter| {
                let count = counter.get();
                counter.set(count + 1);
                if count % 10 == 0 {
                    warn!(
                        "{: <80}",
                        format!("ANNODE MODE IS NOT LIVE: current mode='{}'. All mining disabled.", mining_info.annode_mode)
                    );
                }
            });
            return;
        }
        
        if mining_info.amp != "MINING" {
             MESSAGE_COUNTER2.with(|counter| {
                let count = counter.get();
                counter.set(count + 1);
                if count % 10 == 0 {
                    warn!(
                        "{: <80}",
                        format!("AMP STATUS IS '{}'. All mining disabled.", mining_info.amp)
                    );
                }
            });
            return;
        }


        if self.mining_mode == MiningMode::Share && !self.share_mining_enabled {
            self.scanning = false;

            trace!(
                "GUI_JSON {{ \"type\": \"mining_interrupted\", \"height\": {}, \"reason\": \"SHARE_DISABLED\", \"annode_mode\": \"{}\", \"amp\": \"{}\" }}",
                self.height, mining_info.annode_mode, mining_info.amp
            );
            return; 
        }
        
        let was_mining = self.scanning;
        let can_mine_now = mining_info.annode_mode == "LIVE" && mining_info.amp == "MINING";
        
        if was_mining && !can_mine_now {
            info!("MINING INTERRUPTED: annode status changed - annode_mode: {}, amp: {}", 
                mining_info.annode_mode, mining_info.amp);
            self.scanning = false;
            
            trace!(
                "GUI_JSON {{ \"type\": \"mining_interrupted\", \"height\": {}, \"reason\": \"ANNODE_STATUS_CHANGED\", \"annode_mode\": \"{}\", \"amp\": \"{}\" }}",
                self.height, mining_info.annode_mode, mining_info.amp
            );
        }
        
        let scoop = poc_hashing::calculate_scoop(mining_info.height, &self.generation_signature_bytes);
        self.scoop = scoop;
        
        // Only restart scanning if:
        // 1. We can mine now (annode status is good)
        // 2. We're not already scanning  
        // 3. We haven't submitted a solution for this block yet
        // 4. For SHARE mode: share mining is actually enabled (we already checked above)
        if self.finished_scanning {
            MESSAGE_COUNTER2.with(|counter| {
                let count = counter.get();
                counter.set(count + 1);
                
                if count % 10 == 0 {
                    if !self.submitted_for_this_block {
                        info!("ANNE Miner finished scanning, no result found, waiting for next block...");
                    }
                    else if SUBMISSION_SUCCESS.load(Ordering::Relaxed) {
                        info!("ANNE Miner already submitted solution, waiting for next block...");
                    }
                }
            });
            return;
        }
        //havent finished scanning..
        //not currently scanning, 
        //solo/share is enabled
        //haven't submitted a block yet.
        else if !self.scanning && (self.solo_mining_enabled || self.share_mining_enabled) && !self.submitted_for_this_block {
            self.scanning = true;
            self.sw.restart();
            self.processed_reader_tasks = 0;
            self.last_progress_log = 0.0;
            
            trace!(
                "GUI_JSON {{ \"type\": \"mining_started\", \"height\": {}, \"scoop\": {}, \"mining_mode\": \"{}\", \"is_new_block\": {} }}",
                self.height,
                self.scoop,
                match _miner_mode {
                    MiningMode::Solo => "SOLO",
                    MiningMode::Share => "SHARE",
                },
                is_new_block
            );
            
            if is_new_block {
                info!("MINING re-STARTED at height {}", mining_info.height);
            } 
            else {
                info!("MINING RESUMED: connectivity restored at height {}", mining_info.height);
            }
        }
        

    }
}

#[derive(Copy, Clone)]
pub struct NonceData {
    pub height: u64,
    pub block: u64,
    pub deadline: u64,
    pub nonce: u64,
    pub reader_task_processed: bool,
    pub account_id: u64,
}

pub trait Buffer {
    #[allow(unused)]
    fn get_buffer(&mut self) -> Arc<Mutex<Vec<u8>>>;
    fn get_buffer_for_writing(&mut self) -> Arc<Mutex<Vec<u8>>>;
    #[cfg(feature = "opencl")]
    fn get_gpu_buffers(&self) -> Option<&GpuBuffer>;
    #[cfg(feature = "opencl")]
    fn get_gpu_data(&self) -> Option<Mem>;
    fn unmap(&self);
    fn get_id(&self) -> usize;
}

pub struct CpuBuffer {
    data: Arc<Mutex<Vec<u8>>>,
}

impl CpuBuffer {
    pub fn new(buffer_size: usize) -> Self {
        let layout = Layout::from_size_align(buffer_size, page_size::get()).unwrap();
        let pointer;
        unsafe {
            pointer = std::alloc::alloc(layout);
        }       
        let data: Vec<u8>;
        unsafe {
            data = Vec::from_raw_parts(pointer as *mut u8, buffer_size, buffer_size);
        }
        CpuBuffer {
            data: Arc::new(Mutex::new(data)),
        }
    }
}

impl Buffer for CpuBuffer {
    fn get_buffer(&mut self) -> Arc<Mutex<Vec<u8>>> {
        self.data.clone()
    }
    fn get_buffer_for_writing(&mut self) -> Arc<Mutex<Vec<u8>>> {
        self.data.clone()
    }
    #[cfg(feature = "opencl")]
    fn get_gpu_buffers(&self) -> Option<&GpuBuffer> {
        None
    }
    #[cfg(feature = "opencl")]
    fn get_gpu_data(&self) -> Option<Mem> {
        None
    }
    fn unmap(&self) {}
    fn get_id(&self) -> usize {
        0
    }
}

fn scan_plots(
    plot_dirs: &[PathBuf],
    use_direct_io: bool,
    dummy: bool,
    allowed_account_ids: &std::collections::HashSet<u64>
) -> (HashMap<String, Arc<Vec<Mutex<Plot>>>>, u64) {
    let mut drive_id_to_plots: HashMap<String, Vec<Mutex<Plot>>> = HashMap::new();
    let mut global_capacity: u64 = 0;

    let mut total_plots_found = 0;
    let mut plots_loaded = 0;
    let mut plots_skipped = 0;

    if SHOULD_STOP_MINING.load(Ordering::Relaxed) {
        warn!("Initial plot scanning skipped: ANNODE is not in MINING mode");
        let drive_id_to_plots: HashMap<String, Arc<Vec<Mutex<Plot>>>> = HashMap::new();
        return (drive_id_to_plots, 0);
    }

    for plot_dir in plot_dirs {
        let mut num_plots = 0;
        let mut local_capacity: u64 = 0;

        for entry in read_dir(plot_dir).unwrap() {
            let file = entry.unwrap().path();

            if !file.is_file() {
                continue;
            }

            total_plots_found += 1;
            if SHOULD_STOP_MINING.load(Ordering::Relaxed) {
                warn!("Plot scanning interrupted: ANNODE is not in MINING mode");
                break;
            }
            if let Some(file_name) = file.file_name().and_then(|n| n.to_str()) {
                if let Some(first_underscore) = file_name.find('_') {
                    let account_id_str = &file_name[..first_underscore];
                    if let Ok(account_id) = account_id_str.parse::<u64>() {
                        if allowed_account_ids.contains(&account_id) {
                            match Plot::new(&file, use_direct_io, dummy) {
                                Ok(p) => {
                                    let drive_id = get_device_id(
                                        &file.to_str().unwrap().to_string()
                                    );
                                    let plots = drive_id_to_plots
                                        .entry(drive_id)
                                        .or_insert(Vec::new());

                                    local_capacity += p.meta.nonces as u64;
                                    plots.push(Mutex::new(p));
                                    num_plots += 1;
                                    plots_loaded += 1;
                                }
                                Err(e) => {
                                    debug!("Failed to load plot {}: {}", file_name, e);
                                }
                            }
                        } else {
                            plots_skipped += 1;
                            trace!(
                                "Skipping plot file for non-configured account ID: {} (account ID: {})",
                                file_name,
                                account_id
                            );
                        }
                    } else {
                        trace!("Skipping file with invalid account ID format: {}", file_name);
                    }
                } else {
                    trace!("Skipping file without underscore in name: {}", file_name);
                }
            }
        }

        if SHOULD_STOP_MINING.load(Ordering::Relaxed) {
            warn!("Plot scanning stopped after directory: ANNODE is not in MINING mode");
            break;
        }

        info!(
            "path={}, files={}, size={:.4} TiB",
            plot_dir.to_str().unwrap(),
            num_plots,
            (local_capacity as f64) / 4.0 / 1024.0 / 1024.0
        );

        global_capacity += local_capacity;
        if num_plots == 0 {
            warn!("no plots for configured account IDs in {}", plot_dir.to_str().unwrap());
        }
    }

    let drive_id_to_plots: HashMap<String, Arc<Vec<Mutex<Plot>>>> = drive_id_to_plots
        .drain()
        .map(|(drive_id, mut plots)| {
            plots.sort_by_key(|p| {
                let m = p.lock().unwrap().fh.metadata().unwrap();
                -FileTime::from_last_modification_time(&m).unix_seconds()
            });
            (drive_id, Arc::new(plots))
        })
        .collect();

    info!(
        "plot files loaded: total drives={}, total capacity={:.4} TiB",
        drive_id_to_plots.len(),
        (global_capacity as f64) / 4.0 / 1024.0 / 1024.0
    );

    info!(
        "plot filtering: found={}, loaded={}, skipped={}, account_ids={:?}",
        total_plots_found,
        plots_loaded,
        plots_skipped,
        allowed_account_ids.iter().collect::<Vec<_>>()
    );

    (drive_id_to_plots, global_capacity * 64)
}


impl Miner {
    #[allow(unused)]
    pub fn stop_reading(&self) {
        self.reader.stop_reading();
    }
    
    pub fn new(cfg: Cfg, executor: Handle) -> Miner {
    

        let mining_mode = cfg.get_mining_mode();

         let allowed_account_ids: HashSet<u64> = cfg.account_id_to_secret_phrase
            .keys()
            .copied()
            .collect();
        
        info!("Filtering plots for account IDs: {:?}", allowed_account_ids.iter().collect::<Vec<_>>());
        
        let (drive_id_to_plots, total_size) =
            scan_plots(&cfg.plot_dirs, cfg.hdd_use_direct_io, cfg.benchmark_cpu(), &allowed_account_ids);

        let cpu_threads = cfg.cpu_threads;
        let cpu_worker_task_count = cfg.cpu_worker_task_count;

        let cpu_buffer_count = cpu_worker_task_count
            + if cpu_worker_task_count > 0 {
                cpu_threads
            } else {
                0
            };

        let reader_thread_count = if cfg.hdd_reader_thread_count == 0 {
            drive_id_to_plots.len()
        } else {
            cfg.hdd_reader_thread_count
        };

        #[cfg(feature = "opencl")]
        let gpu_worker_task_count = cfg.gpu_worker_task_count;
        #[cfg(feature = "opencl")]
        let gpu_threads = cfg.gpu_threads;
        #[cfg(feature = "opencl")]
        let gpu_buffer_count = if gpu_worker_task_count > 0 {
            if cfg.gpu_async {
                gpu_worker_task_count + 2 * gpu_threads
            } else {
                gpu_worker_task_count + gpu_threads
            }
        } else {
            0
        };
        #[cfg(feature = "opencl")]
        {
            info!(
                "reader-threads={}, CPU-threads={}, GPU-threads={}",
                reader_thread_count, cpu_threads, gpu_threads,
            );

            info!(
                "CPU-buffer={}(+{}), GPU-buffer={}(+{})",
                cpu_worker_task_count,
                if cpu_worker_task_count > 0 {
                    cpu_threads
                } else {
                    0
                },
                gpu_worker_task_count,
                if gpu_worker_task_count > 0 {
                    if cfg.gpu_async {
                        2 * gpu_threads
                    } else {
                        gpu_threads
                    }
                } else {
                    0
                }
            );

            {
                if cpu_threads * cpu_worker_task_count + gpu_threads * gpu_worker_task_count == 0 {
                    error!("CPU, GPU: no active workers. Check thread and task configuration. Shutting down...");
                    process::exit(0);
                }
            }
        }

        #[cfg(not(feature = "opencl"))]
        {
            info!(
                "reader-threads={} CPU-threads={}",
                reader_thread_count, cpu_threads
            );
            info!("CPU-buffer={}(+{})", cpu_worker_task_count, cpu_threads);
            {
                if cpu_threads * cpu_worker_task_count == 0 {
                    error!(
                    "CPU: no active workers. Check thread and task configuration. Shutting down..."
                );
                    process::exit(0);
                }
            }
        }

        #[cfg(not(feature = "opencl"))]
        let buffer_count = cpu_buffer_count;
        #[cfg(feature = "opencl")]
        let buffer_count = cpu_buffer_count + gpu_buffer_count;
        let buffer_size_cpu = cfg.cpu_nonces_per_cache * SCOOP_SIZE as usize;
        let (tx_empty_buffers, rx_empty_buffers) =
            crossbeam_channel::bounded(buffer_count as usize);
        let (tx_read_replies_cpu, rx_read_replies_cpu) =
            crossbeam_channel::bounded(cpu_buffer_count);

        #[cfg(feature = "opencl")]
        let mut tx_read_replies_gpu = Vec::new();
        #[cfg(feature = "opencl")]
        let mut rx_read_replies_gpu = Vec::new();
        #[cfg(feature = "opencl")]
        let mut gpu_contexts = Vec::new();
        #[cfg(feature = "opencl")]
        {
            for _ in 0..gpu_threads {
                let (tx, rx) = crossbeam_channel::unbounded();
                tx_read_replies_gpu.push(tx);
                rx_read_replies_gpu.push(rx);
            }

            for _ in 0..gpu_threads {
                gpu_contexts.push(Arc::new(GpuContext::new(
                    cfg.gpu_platform,
                    cfg.gpu_device,
                    cfg.gpu_nonces_per_cache,
                    if cfg.benchmark_io() {
                        false
                    } else {
                        cfg.gpu_mem_mapping
                    },
                )));
            }
        }

        for _ in 0..cpu_buffer_count {
            let cpu_buffer = CpuBuffer::new(buffer_size_cpu);
            tx_empty_buffers
                .send(Box::new(cpu_buffer) as Box<dyn Buffer + Send>)
                .unwrap();
        }

        #[cfg(feature = "opencl")]
        for (i, context) in gpu_contexts.iter().enumerate() {
            for _ in 0..(gpu_buffer_count / gpu_threads
                + if i == 0 {
                    gpu_buffer_count % gpu_threads
                } else {
                    0
                })
            {
                let gpu_buffer = GpuBuffer::new(&context.clone(), i + 1);
                tx_empty_buffers
                    .send(Box::new(gpu_buffer) as Box<dyn Buffer + Send>)
                    .unwrap();
            }
        }

        let (tx_nonce_data, rx_nonce_data) = mpsc::channel(buffer_count);

        thread::spawn({
            create_cpu_worker_task(
                cfg.benchmark_io(),
                new_thread_pool(cpu_threads, cfg.cpu_thread_pinning),
                rx_read_replies_cpu.clone(),
                tx_empty_buffers.clone(),
                tx_nonce_data.clone(),
            )
        });

        #[cfg(feature = "opencl")]
        for i in 0..gpu_threads {
            if cfg.gpu_async {
                thread::spawn({
                    create_gpu_worker_task_async(
                        cfg.benchmark_io(),
                        rx_read_replies_gpu[i].clone(),
                        tx_empty_buffers.clone(),
                        tx_nonce_data.clone(),
                        gpu_contexts[i].clone(),
                        drive_id_to_plots.len(),
                    )
                });
            } else {
                #[cfg(feature = "opencl")]
                thread::spawn({
                    create_gpu_worker_task(
                        cfg.benchmark_io(),
                        rx_read_replies_gpu[i].clone(),
                        tx_empty_buffers.clone(),
                        tx_nonce_data.clone(),
                        gpu_contexts[i].clone(),
                    )
                });
            }
        }

        #[cfg(feature = "opencl")]
        let tx_read_replies_gpu = Some(tx_read_replies_gpu);
        #[cfg(not(feature = "opencl"))]
        let tx_read_replies_gpu = None;

        Miner {
            reader_task_count: drive_id_to_plots.len(),
            reader: Reader::new(
                drive_id_to_plots,
                total_size,
                reader_thread_count,
                rx_empty_buffers,
                tx_empty_buffers,
                tx_read_replies_cpu,
                tx_read_replies_gpu,
                cfg.show_progress,
                cfg.show_drive_stats,
                cfg.cpu_thread_pinning,
                cfg.benchmark_cpu()
            ),
            rx_nonce_data,
            target_deadline: cfg.target_deadline,
            account_id_to_target_deadline: cfg.account_id_to_target_deadline,
            request_handler: RequestHandler::new(
                cfg.url,
                cfg.account_id_to_secret_phrase,
                cfg.timeout,
                (total_size * 4 / 1024 / 1024) as usize,
                cfg.send_proxy_details,
                cfg.additional_headers,
                executor.clone(),
            ),
            state: Arc::new(Mutex::new(State::new())),

            get_mining_info_interval: max(1000, cfg.get_mining_info_interval),
            executor,
            wakeup_after: cfg.hdd_wakeup_after * 1000,
            submit_only_best : cfg.submit_only_best,
            mining_mode,
        }
    }

    pub async fn run(self) {
        use tokio::time::{sleep, Duration};
        let request_handler = self.request_handler.clone();
        let total_size = self.reader.total_size;

        let reader = Arc::new(Mutex::new(self.reader));
        let reader_for_second_closure = reader.clone();
        let state = self.state.clone();

        let get_mining_info_interval = self.get_mining_info_interval;
        #[allow(unused)]
        let wakeup_after = self.wakeup_after;
        let mining_mode = self.mining_mode;  
        tokio::spawn(async move {
            info!("→ Interval task started");
            thread_local! {
                static MESSAGE_COUNTER: Cell<u32> = Cell::new(0);
            }
            Interval::new_interval(Duration::from_millis(get_mining_info_interval))
                .for_each(move |_| {
                    let state = state.clone();
                    let reader = reader.clone();
                    let request_handler = request_handler.clone();
                    async move {
                        let mining_info = request_handler.get_mining_info();
                        
                        match mining_info.await {
                            Ok(mining_info) => {
                                SHOULD_STOP_MINING.store(mining_info.amp != "MINING", Ordering::Relaxed);
                                let mut state = state.lock().unwrap();
                                state.first = false;
                                if state.outage {
                                    error!("{: <80}", "outage resolved.");
                                    state.outage = false;
                                }
                                if SUBMISSION_SUCCESS.load(Ordering::Relaxed) {
                                    info!("✅ SUBMISSION CONFIRMED: annode has solution for height {}", state.height);
                                    SUBMISSION_SUCCESS.store(false, Ordering::Relaxed);
                                    state.submitted_for_this_block = true;
                                    state.scanning = false;
                                }


                                if SUBMISSION_FAILED.load(Ordering::Relaxed) {
                                    // other logs will inform
                                  //  info!("⚠️ ANNE Miner submission likely succeeded but request timed out and we didn't get confirmation. Check if annode has it.");
                                    SUBMISSION_FAILED.store(false, Ordering::Relaxed);
                                    // don't set the state here, let the atomics handle it
                                    // state.submitted_for_this_block = false;
                                }

                                //new height detected.
                                if mining_info.height != state.height {
                                    // This is a new block - start mining
                                    state.update_mining_info(&mining_info, mining_mode);

                                    // Only start reading if scanning is enabled (which means mining is possible)
                                    if state.scanning {
                                        let mode_str = match mining_mode {
                                            MiningMode::Solo => "SOLO",
                                            MiningMode::Share => "SHARE",
                                        };
                                        info!("{: <80}", format!("MINING ACTIVE: Starting plot scanning ({})", mode_str));
                                        reader.lock().unwrap().start_reading(
                                            mining_info.height,
                                            state.block,
                                            state.scoop,
                                            &Arc::new(state.generation_signature_bytes),
                                        );
                                        drop(state);
                                    } 
                                    else {
                                        let mode_str = match mining_mode {
                                            MiningMode::Solo => "SOLO",
                                            MiningMode::Share => "SHARE",
                                        };
                                        info!("{: <80}", format!("ENERGY SAVING: Plot scanning skipped - {} mining disabled", mode_str));
                                        drop(state);
                                    }
                                } 

                                else {
                                    let was_mining = state.scanning;
                                    state.update_mining_info(&mining_info, mining_mode);

                                    // failed coms, notified elsewhere, remain silent
                                     if SUBMISSION_FAILED.load(Ordering::Relaxed) {
                                        SUBMISSION_FAILED.store(false, Ordering::Relaxed);
                                       // state.submitted_for_this_block=false;
                                    }

                                    //in SHARE mode, already submit this block, or finished scanning (with no block)
                                    if state.mining_mode == MiningMode::Share && (state.submitted_for_this_block || state.finished_scanning==true) {
                                        // In share mode, if we've already submitted for this block, don't restart mining
                                        // even if annode status says we can mine - wait for the next block
                                        state.scanning = false;
                                        
                                        MESSAGE_COUNTER.with(|counter| {
                                            let count = counter.get();
                                            counter.set(count + 1);
                                            if count % 10 == 0 {
                                                if state.submitted_for_this_block {
                                                    debug!("SHARE MODE: Already submitted SHARE for this height {}, waiting for new block", mining_info.height);
                                                }
                                                else {
                                                    info!("SHARE MODE: finished scanning, no solution found for this height {}, waiting for new block", mining_info.height);
                                                }
                                            }
                                        });

                                        drop(state);
                                        return;
                                    }

                                    else if state.mining_mode == MiningMode::Solo && (state.submitted_for_this_block || state.finished_scanning==true)  {
                                        // In share mode, if we've already submitted for this block, don't restart mining
                                        // even if annode status says we can mine - wait for the next block
                                        state.scanning = false;
                                        
                                        MESSAGE_COUNTER.with(|counter| {
                                            let count = counter.get();
                                            counter.set(count + 1);
                                            if count % 10 == 0 {
                                                if state.submitted_for_this_block {
                                                    debug!("SOLO MODE: Already submitted SOLO for this height {}, waiting for new block", mining_info.height);
                                                }
                                                else {
                                                    info!("SOLO MODE: Finished scanning, no solution found at height {}, waiting for new block", mining_info.height);
                                                }
                                            }
                                        });

                                        drop(state);
                                        return; 
                                    }
                                    else if state.mining_mode == MiningMode::Share && state.share_mining_enabled==false && !mining_info.share_mining_ok{
                                        state.scanning = false;                              
                                        MESSAGE_COUNTER.with(|counter| {
                                            let count = counter.get();
                                            counter.set(count + 1);
                                            if count % 10 == 0 {
                                                info!("SHARE MODE: no share mining available this at height {}", mining_info.height);
                                            }
                                        });
                                        drop(state);
                                        return;
                                    }
                                    
                                    let can_mine_now = mining_info.annode_mode == "LIVE" && mining_info.amp == "MINING";
                                    
                                    state.solo_mining_enabled = can_mine_now;
                                    state.share_mining_enabled = can_mine_now && mining_info.share_mining_ok;
                                    state.scanning = state.solo_mining_enabled || state.share_mining_enabled;
                                    
                                    if state.mining_mode == MiningMode::Share && !state.share_mining_enabled {
                                        info!("EMERGENCY STOP: Share mining disabled - preventing plot scan");
                                        state.scanning = false;
                                        state.submitted_for_this_block = true;
                                        reader.lock().unwrap().stop_reading();
                                    }
                                    
                                    if !was_mining && state.scanning {
                                        info!("MINING AVAILABLE again");

                                        let mode_str = match mining_mode {
                                            MiningMode::Solo => "SOLO",
                                            MiningMode::Share => "SHARE",
                                        };
                                        info!("{: <80}", format!("MINING ACTIVE: Starting plot scanning ({})", mode_str));
                                        
                                        reader.lock().unwrap().start_reading(
                                            mining_info.height,
                                            state.block,
                                            state.scoop,
                                            &Arc::new(state.generation_signature_bytes),
                                        );
                                        drop(state);
                                    } 
                                    else if was_mining && !state.scanning {

                                        info!("MINING INTERRUPTED: annode status changed to non-mining mode");
                                        drop(state);
                                    }
                                    else {
                                        drop(state);
                                    }
                                }
                            }
                            _ => {
                                let mut state = state.lock().unwrap();
                                if state.first {
                                    error!(
                                        "{: <80}",
                                        "error getting mining info, please check if annode is running or its config"
                                    );
                                    state.first = false;
                                    state.outage = true;
                                } else if !state.outage {
                                        error!(
                                            "{: <80}",
                                            "error getting mining info => connection outage..."
                                        );
                                    state.outage = true;
                                }
                            }
                        }
                    }
                    })
                .await; 
        });

        // only start submitting nonces after a while
        let mut best_nonce_data = NonceData {
            height: 0,
            block: 0,
            deadline: 0,
            nonce: 0,
            reader_task_processed: false,
            account_id: 0,
        };

        let target_deadline = self.target_deadline;
        let account_id_to_target_deadline = self.account_id_to_target_deadline;
        let request_handler = self.request_handler.clone();
        let state = self.state.clone();
        let reader_task_count = self.reader_task_count;
        let inner_submit_only_best = self.submit_only_best;

        let mode_str = match self.mining_mode {
            MiningMode::Solo => "SOLO",
            MiningMode::Share => "SHARE",
        };
    

        self.executor.clone().spawn(async move { 
            let reader = reader_for_second_closure;
            ReceiverStream::new(self.rx_nonce_data)
                .for_each(move |nonce_data| {
                    let reader = reader.clone();
                    let mut state = state.lock().unwrap();
                    
                    if state.mining_mode == MiningMode::Share && !state.share_mining_enabled {
                        info!("EMERGENCY STOP: Share mining disabled - stopping scanning and submissions");
                        state.scanning = false;
                        state.submitted_for_this_block = true;
                        reader.lock().unwrap().stop_reading();
                        return futures_util::future::ready(());
                    }
                    
                    trace!("🤔 DEBUG: Received nonce_data - account_id: {}, nonce: {}, deadline: {}, reader_task_processed: {}, current_height: {}", 
                        nonce_data.account_id, nonce_data.nonce, nonce_data.deadline / 2000000000, 
                        nonce_data.reader_task_processed, state.height);

                    let deadline = nonce_data.deadline / 2000000000;

                    if state.height == nonce_data.height {
                        let best_deadline = *state
                            .account_id_to_best_deadline
                            .get(&nonce_data.account_id)
                            .unwrap_or(&u64::MAX);
                            
                        if best_deadline > deadline
                            && deadline
                                < min(
                                    state.annode_target_deadline,
                                    *(account_id_to_target_deadline
                                        .get(&nonce_data.account_id)
                                        .unwrap_or(&target_deadline)),
                                ) {

                            info!("found a better solution - {} miner: {}, nonce: {}, deadline-legacy: {}", 
                                mode_str, nonce_data.account_id, nonce_data.nonce, deadline);
                            
                            trace!(
                                "GUI_JSON {{ \"type\": \"solution_found\", \"height\": {}, \"account_id\": {}, \"nonce\": {}, \"deadline\": {}, \"miner_mode\": \"{}\" }}",
                                nonce_data.height,
                                nonce_data.account_id,
                                nonce_data.nonce,
                                deadline,
                                mode_str
                            );
                            
                            state
                                .account_id_to_best_deadline
                                .insert(nonce_data.account_id, deadline);
                            
                            if state.mining_mode == MiningMode::Share {
                                if inner_submit_only_best {
                                    // 🤔 DEBUG: Log when updating best_nonce_data
                                    debug!("🤔 DEBUG: SHARE - Updating best_nonce_data from {} to {} (nonce: {} -> {})", 
                                        best_nonce_data.deadline / 2000000000, 
                                        deadline,
                                        best_nonce_data.nonce,
                                        nonce_data.nonce);
                                    best_nonce_data = nonce_data.clone();
                                } 
                                else {
                                    info!("SHARE mining: submitting first solution found (submit_only_best = false)");
                                    
                                    state.scanning = false;
                                    state.submitted_for_this_block = true;
                                    
                                    request_handler.submit_nonce(
                                        nonce_data.account_id,
                                        nonce_data.nonce,
                                        nonce_data.height,
                                        nonce_data.block,
                                        nonce_data.deadline,
                                        deadline,
                                        state.generation_signature_bytes,
                                    );
                                }
                            } 
                            else {
                                if inner_submit_only_best {
                                    debug!("🤔 DEBUG: SOLO - Updating best_nonce_data from {} to {} (nonce: {} -> {})", 
                                        best_nonce_data.deadline / 2000000000, 
                                        deadline,
                                        best_nonce_data.nonce,
                                        nonce_data.nonce);
                                    best_nonce_data = nonce_data.clone();
                                }
                                else {
                                     info!(
                                        "SOLO mining: immediately submitting better solution nonce={}, deadline={}",
                                        nonce_data.nonce,
                                        deadline
                                );
                                    request_handler.submit_nonce(
                                        nonce_data.account_id,
                                        nonce_data.nonce,
                                        nonce_data.height,
                                        nonce_data.block,
                                        nonce_data.deadline,
                                        deadline,
                                        state.generation_signature_bytes,
                                    );
                                }
                            }
                        }

                        if nonce_data.reader_task_processed {
                            debug!("🤔 DEBUG: Reader task completion received - task count: {}/{}", 
                                state.processed_reader_tasks + 1, reader_task_count);
                            
                            state.processed_reader_tasks += 1;
                            
                            let elapsed_ms = state.sw.elapsed_ms();
                            let speed_mb_s = if elapsed_ms > 0 {
                                total_size as f64 * 1000.0 / 1024.0 / 1024.0 / elapsed_ms as f64
                            } else {
                                0.0
                            };
                            let progress_percent = (state.processed_reader_tasks as f64 / reader_task_count as f64) * 100.0;
                            
                            if state.processed_reader_tasks == reader_task_count || 
                            (progress_percent as i32 / 10) > (state.last_progress_log as i32 / 10) {
                                
                                trace!(
                                    "GUI_JSON {{ \"type\": \"scan_progress\", \"height\": {}, \"progress\": {:.2}, \"speed_mb_s\": {:.2} }}",
                                    state.height,
                                    progress_percent,
                                    speed_mb_s
                                );
                                
                                state.last_progress_log = progress_percent;
                            }
                            
                            if state.processed_reader_tasks == reader_task_count {
                                state.finished_scanning = true;

                                info!(
                                    "{: <80}",
                                    format!(
                                        "✅ Round finished: roundtime={}ms, speed={:.2}MiB/s",
                                        state.sw.elapsed_ms(),
                                        speed_mb_s
                                    )
                                );

                                debug!("🤔 DEBUG: Finished scanning - best_nonce_data: height={}, nonce={}, deadline={}, current_height={}", 
                                    best_nonce_data.height, best_nonce_data.nonce, best_nonce_data.deadline / 2000000000, state.height);

                                trace!(
                                    "GUI_JSON {{ \"type\": \"scan_complete\", \"height\": {}, \"roundtime_ms\": {}, \"speed_mb_s\": {:.2}, \"submitted\": {}, \"best_deadline\": {}, \"best_nonce\": {}, \"miner_mode\": \"{}\" }}",
                                    state.height,
                                    state.sw.elapsed_ms(),
                                    speed_mb_s,
                                    state.submitted_for_this_block,
                                    best_nonce_data.deadline / 2000000000,
                                    best_nonce_data.nonce,
                                    mode_str
                                );

                                if best_nonce_data.height == state.height {
                                    let deadline = best_nonce_data.deadline / 2000000000;
                                    
                                    if state.mining_mode == MiningMode::Share && inner_submit_only_best {
                                        info!("SHARE mining: submitting best solution after full scan: nonce={}, deadline={}",
                                            best_nonce_data.nonce,
                                            deadline);
                                            
                                        state.submitted_for_this_block = true;
                                        request_handler.submit_nonce(
                                            best_nonce_data.account_id,
                                            best_nonce_data.nonce,
                                            best_nonce_data.height,
                                            best_nonce_data.block,
                                            best_nonce_data.deadline,
                                            deadline,
                                            state.generation_signature_bytes,
                                        );
                                    }
                                    else if state.mining_mode == MiningMode::Solo && inner_submit_only_best {
                                           info!("SOLO mining: submitting best solution after full scan: nonce={}, deadline={}",
                                            best_nonce_data.nonce,
                                            deadline);
                                        
                                        state.submitted_for_this_block = true;
                                        request_handler.submit_nonce(
                                            best_nonce_data.account_id,
                                            best_nonce_data.nonce,
                                            best_nonce_data.height,
                                            best_nonce_data.block,
                                            best_nonce_data.deadline,
                                            deadline,
                                            state.generation_signature_bytes,
                                        );
                                    }
                                } 
                                else if best_nonce_data.height != state.height {
                                    debug!("🤔 DEBUG: best_nonce_data.height ({}) doesn't match current height ({}) - not submitting", 
                                        best_nonce_data.height, state.height);
                                }

                                if state.mining_mode == MiningMode::Share {
                                    state.scanning = false;
                                    if !state.submitted_for_this_block {
                                        info!("SHARE mining: no valid solution found in this round");
                                    }
                                }
                                
                                state.sw.restart();
                                state.scanning = false;
                            }
                        }
                    }
                    else {
                        // 🤔 DEBUG: Reset best_nonce_data when we get data for a different height
                        if nonce_data.height != best_nonce_data.height && nonce_data.height != 0 {
                            debug!("🤔 DEBUG: Resetting best_nonce_data - new height: {}, old height: {}", 
                                nonce_data.height, best_nonce_data.height);
                        best_nonce_data = NonceData {
                            height: 0,
                            block: 0,
                            deadline: 0,
                            nonce: 0,
                            reader_task_processed: false,
                            account_id: 0,
                        };
                        }
                    }
                    futures_util::future::ready(())
                })
                .await;
        });

      loop {
        sleep(Duration::from_secs(60)).await;
        }
    }
}