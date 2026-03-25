#![warn(unused_extern_crates)]

#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate cfg_if;

#[macro_use]
extern crate log;

mod com;
mod config;
mod cpu_worker;
mod future;
mod logger;
mod miner;
mod plot;
mod poc_hashing;
mod reader;
mod requests;
mod shabal256;
mod utils;

#[cfg(feature = "opencl")]
mod gpu_worker;
#[cfg(feature = "opencl")]
mod gpu_worker_async;
#[cfg(feature = "opencl")]
mod ocl;

use crate::config::load_cfg;
use crate::miner::Miner;
use clap::{ Arg, Command };
use std::process;
use std::env;
use std::io;
#[allow(unused)]
use std::io::Write;

extern "C" {
    pub fn init_shabal_all() -> ();

    pub fn find_best_deadline_sph(
        scoops: *mut std::os::raw::c_char,
        nonce_count: u64,
        gensig: *mut std::os::raw::c_char,
        best_deadline: *mut u64,
        best_offset: *mut u64
    );
}

fn init_shabal() {
    static INIT: std::sync::Once = std::sync::Once::new();

    INIT.call_once(|| {
        unsafe {
            init_shabal_all();
        }
        info!("ANNE Miner initialized with automatic CPU detection");
    });
}

#[cfg(unix)]
fn is_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

#[cfg(target_os = "linux")]
fn get_terminal_command() -> Option<(String, Vec<String>)> {
    if
        let Ok(output) = std::process::Command
            ::new("xdg-mime")
            .args(&["query", "default", "x-scheme-handler/terminal"])
            .output()
    {
        if output.status.success() {
            let default_term = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !default_term.is_empty() {
                if default_term.ends_with(".desktop") {
                    let desktop_path = format!("/usr/share/applications/{}", default_term);
                    if let Ok(content) = std::fs::read_to_string(&desktop_path) {
                        for line in content.lines() {
                            if line.starts_with("Exec=") {
                                let exec_line = &line[5..];
                                let parts: Vec<&str> = exec_line.split_whitespace().collect();
                                if !parts.is_empty() {
                                    let executable = parts[0].to_string();
                                    let mut args = Vec::new();
                                    for &arg in &parts[1..] {
                                        if !arg.contains("%") {
                                            args.push(arg.to_string());
                                        }
                                    }
                                    args.push("-e".to_string());
                                    return Some((executable, args));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let desktop_env = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase();

    if desktop_env.contains("xfce") {
        Some(("xfce4-terminal".to_string(), vec!["-x".to_string()]))
    } else if
        desktop_env.contains("gnome") ||
        desktop_env.contains("ubuntu") ||
        desktop_env.contains("pop")
    {
        let gnome_terminals = [
            ("ptyxis", vec!["--"]),
            ("kgx", vec!["-e"]),
            ("gnome-terminal", vec!["--"]),
            ("tilix", vec!["-e"]),
            ("terminator", vec!["-e"]),
        ];

        for (term, args) in &gnome_terminals {
            if which::which(term).is_ok() {
                return Some((
                    term.to_string(),
                    args
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                ));
            }
        }

        let universal = [
            ("alacritty", vec!["-e"]),
            ("kitty", vec!["-e"]),
            ("wezterm", vec!["start", "--"]),
        ];

        for (term, args) in &universal {
            if which::which(term).is_ok() {
                return Some((
                    term.to_string(),
                    args
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                ));
            }
        }

        Some(("xterm".to_string(), vec!["-e".to_string()]))
    } else if desktop_env.contains("kde") || desktop_env.contains("plasma") {
        Some(("konsole".to_string(), vec!["-e".to_string()]))
    } else if desktop_env.contains("mate") {
        Some(("mate-terminal".to_string(), vec!["-x".to_string()]))
    } else if desktop_env.contains("lxde") || desktop_env.contains("lubuntu") {
        Some(("lxterminal".to_string(), vec!["-e".to_string()]))
    } else {
        let all_terminals = [
            ("xfce4-terminal", vec!["-x"]),
            ("konsole", vec!["-e"]),
            ("mate-terminal", vec!["-x"]),
            ("lxterminal", vec!["-e"]),
            ("gnome-terminal", vec!["--"]),

            ("alacritty", vec!["-e"]),
            ("kitty", vec!["-e"]),
            ("wezterm", vec!["start", "--"]),
            ("tilix", vec!["-e"]),
            ("terminator", vec!["-e"]),

            ("xterm", vec!["-e"]),
            ("urxvt", vec!["-e"]),
            ("rxvt", vec!["-e"]),
        ];

        for (term, args) in &all_terminals {
            if which::which(term).is_ok() {
                return Some((
                    term.to_string(),
                    args
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                ));
            }
        }

        Some(("xterm".to_string(), vec!["-e".to_string()]))
    }
}

#[cfg(target_os = "linux")]
fn spawn_linux_terminal() -> Result<(), Box<dyn std::error::Error>> {
    let exe_path = env::current_exe()?;
    let exe_dir = exe_path.parent().unwrap_or_else(|| std::path::Path::new("."));

    let wrapper_script = format!(
        r#"#!/bin/bash
cd "{}"
echo "Starting ANNE Miner..."
echo "------------------------------------------------------------"
sleep 0.5
"{}"
EXIT_CODE=$?
echo ""
echo "------------------------------------------------------------"
if [ $EXIT_CODE -eq 0 ]; then
    echo "Miner completed successfully."
else
    echo "Miner exited with code: $EXIT_CODE"
fi
echo "Press any key to close this window..."
read -n 1 -s
"#,
        exe_dir.display(),
        exe_path.display()
    );

    let temp_dir = env::temp_dir();
    let wrapper_path = temp_dir.join("anne-miner-run.sh");
    std::fs::write(&wrapper_path, wrapper_script)?;

    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&wrapper_path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&wrapper_path, perms)?;

    if let Some((term, mut args)) = get_terminal_command() {
        let has_exec_flag = args.iter().any(|arg| arg == "-e" || arg == "-x" || arg == "--");
        if !has_exec_flag {
            if term == "gnome-terminal" {
                args.push("--".to_string());
            } else if term.contains("terminal") {
                args.push("-e".to_string());
            }
        }

        args.push(wrapper_path.to_string_lossy().into_owned());

        println!("Opening in {}...", term);
        let mut cmd = std::process::Command::new(&term);
        cmd.args(&args);

        let status = cmd.spawn()?;

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let _ = std::fs::remove_file(wrapper_path);
        });

        std::mem::forget(status);
        Ok(())
    } else {
        Err("Could not find a suitable terminal".into())
    }
}

#[cfg(target_os = "macos")]
fn spawn_macos_terminal() -> Result<(), Box<dyn std::error::Error>> {
    let exe_path = env::current_exe()?;
    let exe_dir = exe_path.parent().unwrap_or_else(|| std::path::Path::new("."));

    let mut command = format!("cd '{}' && '{}'", exe_dir.display(), exe_path.display());
    for arg in env::args().skip(1) {
        command.push_str(&format!(" '{}'", arg.replace("'", "'\"'\"'")));
    }
    command.push_str(" && echo '' && read -p 'Press Enter to close...'");

    let script =
        format!("tell application \"Terminal\"
            activate
            do script \"{}\"
        end tell", command);

    std::process::Command::new("osascript").arg("-e").arg(&script).spawn()?;

    Ok(())
}

fn wait_for_keypress() {
    println!("\nPress Enter to exit...");

    #[cfg(windows)]
    {
        use std::io::{ self, Write };
        use std::time::Duration;

        let _ = io::stdout().flush();
        let _ = io::stderr().flush();
        std::thread::sleep(Duration::from_millis(100));
    }

    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
}

fn set_terminal_title() {
    let title = format!("ANNE Miner v{}", env!("CARGO_PKG_VERSION"));

    #[cfg(target_os = "windows")]
    {
        use winapi::um::consoleapi::{GetConsoleMode, SetConsoleMode};
        use winapi::um::wincon::SetConsoleTitleW;
        use winapi::um::winbase::STD_OUTPUT_HANDLE;
        use winapi::um::processenv::GetStdHandle;
        use std::os::windows::ffi::OsStrExt;
        use std::ffi::OsStr;

        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut mode: winapi::shared::minwindef::DWORD = 0;
            
            if GetConsoleMode(handle, &mut mode) != 0 {
                // Disable ENABLE_QUICK_EDIT_MODE (0x0040) and ENABLE_MOUSE_INPUT (0x0010)
                mode &= !0x0040;
                mode &= !0x0010;
                SetConsoleMode(handle, mode);
            }
        }

        let wide: Vec<u16> = OsStr::new(&title).encode_wide().chain(Some(0)).collect();

        unsafe {
            SetConsoleTitleW(wide.as_ptr());
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        print!("\x1b]0;{}\x07", title);
        let _ = std::io::stdout().flush();
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    #[cfg(target_os = "macos")]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let _ = std::env::set_current_dir(exe_dir);
            }
        }
    }
    set_terminal_title();
    let matches = Command::new("anne-miner")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Location of the config file")
                .default_value("config.yaml")
        )
        .arg(
            Arg::new("daemon")
                .short('d')
                .long("daemon")
                .action(clap::ArgAction::SetTrue)
                .help("Run as daemon/service (skip terminal detection)")
        );

    #[cfg(feature = "opencl")]
    let matches = matches.arg(
        Arg::new("opencl").short('o').long("opencl").help("Display OpenCL platforms and devices")
    );

    let matches = matches.get_matches();
    let is_daemon_mode = matches.get_flag("daemon");

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        if !is_terminal() && !is_daemon_mode {
            println!("Opening ANNE Miner in a new terminal window...");

            #[cfg(target_os = "linux")]
            {
                if let Err(e) = spawn_linux_terminal() {
                    if which::which("zenity").is_ok() {
                        let _ = std::process::Command
                            ::new("zenity")
                            .args(
                                &[
                                    "--error",
                                    "--text",
                                    &format!("Failed to open terminal: {}\n\nPlease run from terminal manually.", e),
                                    "--title",
                                    "ANNE Miner",
                                ]
                            )
                            .status();
                    } else {
                        eprintln!("Error: {}. Please run from terminal.", e);
                    }
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
            }

            #[cfg(target_os = "macos")]
            {
                if let Err(e) = spawn_macos_terminal() {
                    let script =
                        format!("display dialog \"Failed to open terminal: {}\n\nPlease run from terminal manually.\" \
                        with title \"ANNE Miner\" with icon caution buttons {{\"OK\"}}", e);
                    let _ = std::process::Command::new("osascript").arg("-e").arg(&script).status();
                }
            }

            std::process::exit(0);
        }
    }

    let result = run_miner(matches).await;

    if let Err(e) = result {
        eprintln!("\n❌ Error: {}", e);

        if !is_daemon_mode {
            wait_for_keypress();
        }

        std::process::exit(1);
    }
}

async fn run_miner(matches: clap::ArgMatches) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        r#"
                           @@@@@@@@@@@@@@@@@@@@@@@@@@@@,
                      @@@@@@@@@@@@@@&%%&@@@@@@@@@@@@@@@@@@@@.
                    @@@@@@@@@,                        @@@@@@@@@@(
                 @@@@@@@@@                                .@@@@@@@@
               @@@@@@@@             %@@@@@@@@                @@@@@@@
             @@@@@@@@            (@@@@@@@@@@@@@.               @@@@@@&
           #@@@@@@@             @@@@@@@@@@@@@@@@.               @@@@@@/
          @@@@@@@@              @@@@@@@@@@@@@@@@/               /@@@@@@
         @@@@@@@@                @@@@@@@@@@@@@@%                 @@@@@@@ 
        @@@@@@@@                    @@@@@@@@@                    @@@@@@@
       @@@@@@@@                                                  @@@@@@@
      @@@@@@@@@                                                  @@@@@@@*
      @@@@@@@@#         @@ @    @&@   @.  @,@  @%  @@@@@@        @@@@@@@@
      @@@@@@@@@        (@  /@   @  @  @.  @  @ @%  @@@(          @@@@@@@@
      @@@@@@@@@@       @    @@  @   @,@.  @   @@%  @@@@@@        @@@@@@@@
       @@@@@@@@@                                                  @@@@@@
        @@@@@@@

    Welcome to ANNE Miner - For use with the ANNODE and ANNE Network.  Let's go!
    "#
    );

    std::thread::sleep(std::time::Duration::from_millis(1500));

    let config = matches
        .get_one::<String>("config")
        .map(|s| s.as_str())
        .unwrap_or("config.yaml");

    let cfg_loaded = match
        std::panic::catch_unwind(|| {
            if !std::path::Path::new(config).exists() {
                panic!("Configuration file '{}' not found!", config);
            }
            load_cfg(config)
        })
    {
        Ok(config) => config,
        Err(panic_err) => {
            let error_msg = if let Some(s) = panic_err.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = panic_err.downcast_ref::<&str>() {
                s.to_string()
            } else {
                format!("Failed to load configuration file '{}'", config)
            };
            return Err(error_msg.into());
        }
    };

    logger::init_logger(&cfg_loaded);
    init_shabal();
    let mining_mode = cfg_loaded.get_mining_mode();
    let mode_str = match mining_mode {
        crate::config::MiningMode::Solo => "SOLO",
        crate::config::MiningMode::Share => "SHARE",
    };

    if cfg!(windows) {
        info!("[MINING: {}]", mode_str);
    } else {
        info!("🚀 MINING: {}", mode_str);
    }

    info!("anne-miner v{}", env!("CARGO_PKG_VERSION"));
    #[cfg(feature = "opencl")]
    info!("GPU extensions: OpenCL");

    #[cfg(feature = "opencl")]
    if matches.contains_id("opencl") {
        ocl::platform_info();
        process::exit(0);
    }



    #[cfg(feature = "opencl")]
    ocl::gpu_info(&cfg_loaded);

    let handle = tokio::runtime::Handle::current();
    let miner = Miner::new(cfg_loaded, handle);
    std::thread::sleep(std::time::Duration::from_secs(1));
    miner.run().await;

    Ok(())
}
