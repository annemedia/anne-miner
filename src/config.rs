use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::u32;

#[derive(Debug, Serialize)]
pub enum Benchmark {
    IO,
    XPU,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MiningMode {
    Solo,
    Share,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Cfg {
    #[serde(default = "default_secret_phrase")]
    pub account_id_to_secret_phrase: HashMap<u64, String>,

    #[serde(default)]
    pub plot_dirs: Vec<PathBuf>,

    pub url: ::url::Url,

    #[serde(default = "default_hdd_reader_thread_count")]
    pub hdd_reader_thread_count: usize,

    #[serde(default = "default_hdd_use_direct_io")]
    pub hdd_use_direct_io: bool,

    #[serde(default = "default_hdd_wakeup_after")]
    pub hdd_wakeup_after: i64,

    #[serde(default = "default_cpu_threads")]
    pub cpu_threads: usize,

    #[serde(default = "default_cpu_worker_task_count")]
    pub cpu_worker_task_count: usize,

    #[serde(default = "default_cpu_nonces_per_cache")]
    pub cpu_nonces_per_cache: usize,

    #[serde(default = "default_cpu_thread_pinning")]
    pub cpu_thread_pinning: bool,

    #[serde(default = "default_gpu_threads")]
    pub gpu_threads: usize,

    #[serde(default = "default_gpu_platform")]
    pub gpu_platform: usize,

    #[serde(default = "default_gpu_device")]
    pub gpu_device: usize,

    #[serde(default = "default_gpu_worker_task_count")]
    pub gpu_worker_task_count: usize,

    #[serde(default = "default_gpu_nonces_per_cache")]
    pub gpu_nonces_per_cache: usize,

    #[serde(default = "default_gpu_mem_mapping")]
    pub gpu_mem_mapping: bool,

    #[serde(default = "default_gpu_async")]
    pub gpu_async: bool,

    #[serde(default = "default_target_deadline")]
    pub target_deadline: u64,

    #[serde(default = "default_account_id_to_target_deadline")]
    pub account_id_to_target_deadline: HashMap<u64, u64>,

    #[serde(default = "default_get_mining_info_interval")]
    pub get_mining_info_interval: u64,

    #[serde(default = "default_timeout")]
    pub timeout: u64,

    #[serde(default = "default_send_proxy_details")]
    pub send_proxy_details: bool,

    #[serde(default = "default_additional_headers")]
    pub additional_headers: HashMap<String, String>,

    #[serde(default = "default_console_log_level")]
    pub console_log_level: String,

    #[serde(default = "default_logfile_log_level")]
    pub logfile_log_level: String,

    #[serde(default = "default_logfile_max_count")]
    pub logfile_max_count: u32,

    #[serde(default = "default_logfile_max_size")]
    pub logfile_max_size: u64,

    #[serde(default = "default_console_log_pattern")]
    pub console_log_pattern: String,

    #[serde(default = "default_logfile_log_pattern")]
    pub logfile_log_pattern: String,

    #[serde(default = "default_show_progress")]
    pub show_progress: bool,

    #[serde(default = "default_show_drive_stats")]
    pub show_drive_stats: bool,

    #[serde(default = "default_submit_only_best")]
    pub submit_only_best: bool,

    pub benchmark_only: Option<Benchmark>,
}

impl<'de> Deserialize<'de> for Benchmark {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str().to_lowercase().as_ref() {
            "i/o" => Benchmark::IO,
            "xpu" => Benchmark::XPU,
            _ => Benchmark::Disabled,
        })
    }
}

fn default_secret_phrase() -> HashMap<u64, String> {
    HashMap::new()
}

#[allow(dead_code)]
fn default_url() -> String {
    "http://localhost:9118".to_string()
}

fn default_hdd_reader_thread_count() -> usize {
    0
}

fn default_hdd_use_direct_io() -> bool {
    true
}

fn default_hdd_wakeup_after() -> i64 {
    240
}

fn default_cpu_threads() -> usize {
    0
}

fn default_cpu_worker_task_count() -> usize {
    0
}

fn default_cpu_nonces_per_cache() -> usize {
    65536
}

fn default_cpu_thread_pinning() -> bool {
    false
}

fn default_gpu_threads() -> usize {
    0
}

fn default_gpu_platform() -> usize {
    0
}

fn default_gpu_device() -> usize {
    0
}

fn default_gpu_worker_task_count() -> usize {
    0
}

fn default_gpu_nonces_per_cache() -> usize {
    1_048_576
}

fn default_gpu_mem_mapping() -> bool {
    false
}

fn default_gpu_async() -> bool {
    false
}

fn default_target_deadline() -> u64 {
    u64::MAX
}

fn default_account_id_to_target_deadline() -> HashMap<u64, u64> {
    HashMap::new()
}

fn default_get_mining_info_interval() -> u64 {
    3000
}

fn default_timeout() -> u64 {
    5000
}

fn default_send_proxy_details() -> bool {
    false
}

fn default_additional_headers() -> HashMap<String, String> {
    HashMap::new()
}

fn default_console_log_level() -> String {
    "Info".to_owned()
}

fn default_logfile_log_level() -> String {
    "Warn".to_owned()
}

fn default_logfile_max_count() -> u32 {
    10
}

fn default_logfile_max_size() -> u64 {
    20 * 1024 * 1024 // 20 MB in bytes
}

fn default_console_log_pattern() -> String {
    "\r{d(%H:%M:%S.%3f%z)} [{h({l}):<5}] [{T}] [{t}] - {M}:{m}{n}".to_owned()
}

fn default_logfile_log_pattern() -> String {
    "\r{d(%Y-%m-%dT%H:%M:%S.%3f%z)} [{h({l}):<5}] [{T}] [{f}:{L}] [{t}] - {M}:{m}{n}".to_owned()
}

fn default_show_progress() -> bool {
    true
}

fn default_show_drive_stats() -> bool {
    false
}

fn default_submit_only_best() -> bool {
    true
}

pub fn load_cfg(config: &str) -> Cfg {
    let cfg_str = fs::read_to_string(config).expect("failed to open config.yaml");
    let cfg: Cfg = serde_yaml::from_str(&cfg_str).expect("failed to parse config.yaml");
    if cfg.hdd_use_direct_io {
        assert!(
            cfg.cpu_nonces_per_cache % 64 == 0 && cfg.gpu_nonces_per_cache % 64 == 0,
            "nonces_per_cache should be divisible by 64 when using direct io"
        );
    }
    validate_cfg(cfg)
}

pub fn validate_cfg(mut cfg: Cfg) -> Cfg {
    let cores = num_cpus::get();
    if cfg.cpu_threads == 0 {
        cfg.cpu_threads = cores;
    } else if cfg.cpu_threads > cores {
        warn!(
            "cpu_threads exceeds number of cores ({}), using ({}) threads",
            cores, cores
        );
        cfg.cpu_threads = cores;
    };

    cfg.plot_dirs = cfg
        .plot_dirs
        .iter()
        .cloned()
        .filter(|plot_dir| {
            if !plot_dir.exists() {
                warn!("path {} does not exist", plot_dir.to_str().unwrap());
                false
            } else if !plot_dir.is_dir() {
                warn!("path {} is not a directory", plot_dir.to_str().unwrap());
                false
            } else {
                true
            }
        })
        .collect();

    cfg
}

impl Cfg {
    pub fn benchmark_cpu(&self) -> bool {
        matches!(self.benchmark_only, Some(Benchmark::XPU))
    }

    pub fn benchmark_io(&self) -> bool {
        matches!(self.benchmark_only, Some(Benchmark::IO))
    }

    pub fn get_mining_mode(&self) -> MiningMode {
        for (_, secret_phrase) in &self.account_id_to_secret_phrase {
            let phrase = secret_phrase.trim().trim_matches('\'').to_uppercase();
            if phrase.ends_with("-SOLO") {
                return MiningMode::Solo;
            } 
            else if phrase.ends_with("-SHARE") {
                return MiningMode::Share;
            }
        }
        MiningMode::Solo
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_load_cfg() {
        let test_dir = "test_data";
        if !fs::metadata(test_dir).is_ok() {
            fs::create_dir_all(test_dir).unwrap();
        }

        let test_config = r#"plot_dirs:
  - "test_data"
url: "http://localhost:9118"
timeout: 5000
"#;

        let config_path = "test_config_temp.yaml";
        fs::write(config_path, test_config).unwrap();
        
        let cfg = load_cfg(config_path);
        assert_eq!(cfg.timeout, 5000);
        assert_eq!(cfg.url, "http://localhost:9118");
        
        let mut expected_path = PathBuf::new();
        expected_path.push("test_data");
        
        assert!(!cfg.plot_dirs.is_empty(), "plot_dirs should not be empty");
        assert_eq!(cfg.plot_dirs, vec![expected_path]);
        
        fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn test_default_values() {
        let test_dir = "test_data";
        if !fs::metadata(test_dir).is_ok() {
            fs::create_dir_all(test_dir).unwrap();
        }

        let test_config = r#"plot_dirs:
  - "test_data"
"#;

        let config_path = "test_config_defaults.yaml";
        fs::write(config_path, test_config).unwrap();
        
        let cfg = load_cfg(config_path);
        
        assert_eq!(cfg.timeout, 5000);
        assert_eq!(cfg.target_deadline, u64::from(u32::MAX));
        assert_eq!(cfg.url, "http://localhost:9118");
        
        fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn test_benchmark_enum() {
        let test_dir = "test_data";
        if !fs::metadata(test_dir).is_ok() {
            fs::create_dir_all(test_dir).unwrap();
        }

        let test_config = r#"plot_dirs:
  - "test_data"
benchmark_only: "IO"
"#;

        let config_path = "test_config_benchmark.yaml";
        fs::write(config_path, test_config).unwrap();
        
        let cfg = load_cfg(config_path);
        
        assert!(cfg.benchmark_io());
        assert!(!cfg.benchmark_cpu());
        
        fs::remove_file(config_path).unwrap();
    }
}