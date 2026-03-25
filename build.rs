extern crate cc;
use std::env;

fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("assets/anneminer.ico");
        resource.set("FileDescription", "ANNE Miner");
        resource.set("ProductName", "ANNE Miner");
        
        resource.set_manifest(r#"
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity version="1.0.0.0" processorArchitecture="*" name="ANNE Miner" type="win32"/>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#);

        if let Err(e) = resource.compile() {
            println!("cargo:warning=Could not embed Windows icon: {}", e);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        let minimum_version = if target_arch == "aarch64" { "11.0" } else { "10.12" };
        
        let info_plist = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>ANNE Miner</string>
    <key>CFBundleDisplayName</key>
    <string>ANNE Miner</string>
    <key>CFBundleIdentifier</key>
    <string>network.anne.miner</string>
    <key>CFBundleVersion</key>
    <string>1.0.0</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CFBundleExecutable</key>
    <string>anne-miner</string>
    <key>CFBundleIconFile</key>
    <string>anneminer.icns</string>
    <key>LSMinimumSystemVersion</key>
    <string>{}</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSUIElement</key>
    <true/>
</dict>
</plist>"#, minimum_version);
        
        let out_dir = env::var("OUT_DIR").unwrap();
        let plist_path = std::path::Path::new(&out_dir).join("Info.plist");
        
        if let Err(e) = std::fs::write(&plist_path, info_plist) {
            println!("cargo:warning=Failed to write Info.plist: {}", e);
        }
        
        env::set_var("MACOSX_DEPLOYMENT_TARGET", minimum_version);
    }

    let target = env::var("TARGET").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let is_android = target.contains("android");
    let is_msvc = target.contains("msvc");
    let is_gnu = target.contains("gnu");

    println!("cargo:warning=Building for: {}", target);
    println!("cargo:warning=Arch: {}, OS: {}, Env: {}", target_arch, target_os, target_env);

    let create_base_config = || {
        let mut config = cc::Build::new();
        
        if is_msvc {
            config
                .flag("/O2")
                .flag("/Oi")
                .flag("/Ot")
                .flag("/Oy")
                .flag("/GT")
                .flag("/GL")
                .flag("/D_CRT_SECURE_NO_WARNINGS");
        } else {
            config.flag("-std=c99");
        
            let is_cross_compiling = env::var("HOST").ok() != env::var("TARGET").ok();
            if !is_cross_compiling && !is_android {
                config.flag("-mtune=native");
            }
            
            if target_os == "macos" {
                config.flag("-Wno-deprecated-declarations");
                let deployment_target = if target_arch == "aarch64" { "11.0" } else { "10.12" };
                config.flag(&format!("-mmacosx-version-min={}", deployment_target));
            }
            
            if target_os == "windows" && !is_msvc {
                config
                    .flag("-Wno-unused-parameter")
                    .flag("-Wno-unused-variable")
                    .flag("-Wno-unused-function");
            }
        }
        config
    };

    let mut base_config = create_base_config();
    base_config
        .file("src/c/sph_shabal.c")
        .file("src/c/shabal.c")
        .file("src/c/common.c")
        .file("src/c/shabal_dispatch.c");
    
    
    base_config.compile("shabal_base");

    if target_arch == "x86_64" {
        let mut sse2_config = create_base_config();
        if !is_msvc {
            sse2_config.flag("-msse2").flag("-msse");
        }
        sse2_config
            .file("src/c/mshabal_128_sse2.c")
            .file("src/c/shabal_sse2.c")
            .file("src/c/common.c");
        
        sse2_config.compile("shabal_sse2");

        let mut avx_config = create_base_config();
        if is_msvc {
            avx_config.flag("/arch:AVX");
        } else {
            avx_config.flag("-mavx").flag("-mxsave");
        }
        avx_config
            .file("src/c/mshabal_128_avx.c")
            .file("src/c/shabal_avx.c")
            .file("src/c/common.c");
        
        
        avx_config.compile("shabal_avx");

        let mut avx2_config = create_base_config();
        if is_msvc {
            avx2_config.flag("/arch:AVX2");
        } else {
            avx2_config.flag("-mavx2").flag("-mfma");
        }
        avx2_config
            .file("src/c/mshabal_256_avx2.c")
            .file("src/c/shabal_avx2.c")
            .file("src/c/common.c");
        
        
        avx2_config.compile("shabal_avx2");

        if target_os != "macos" && 
           std::path::Path::new("src/c/shabal_avx512f.c").exists() {
            
            let mut avx512_config = create_base_config();
            if is_msvc {
                avx512_config.flag("/arch:AVX512");
            } else {
                avx512_config.flag("-mavx512f");
            }
            avx512_config
                .file("src/c/mshabal_512_avx512f.c")
                .file("src/c/shabal_avx512f.c")
                .file("src/c/common.c");
            
            avx512_config.compile("shabal_avx512f");
        }
    } else if target_arch == "aarch64" || target_arch == "arm" {
        if std::path::Path::new("src/c/shabal_neon.c").exists() {
            let mut neon_config = create_base_config();
            if !is_msvc && target_arch == "arm" && target_os != "android" {
                neon_config.flag("-mfpu=neon");
            }
            neon_config
                .file("src/c/mshabal_128_neon.c")
                .file("src/c/shabal_neon.c")
                .file("src/c/common.c");
            
            neon_config.compile("shabal_neon");
        }
    }

    if is_gnu {
        match target_os.as_str() {
            "linux" | "freebsd" => {
                println!("cargo:rustc-link-lib=rt");
            },
            "windows" => {
                println!("cargo:rustc-link-lib=winmm");
                println!("cargo:rustc-link-lib=ws2_32");
                println!("cargo:rustc-link-lib=user32");
                println!("cargo:rustc-link-lib=gdi32");
                println!("cargo:rustc-link-lib=shell32");
                println!("cargo:rustc-link-lib=ole32");
                println!("cargo:rustc-link-lib=uuid");
                
                println!("cargo:rustc-link-arg=-static");
                println!("cargo:rustc-link-arg=-Wl,--subsystem,console");
                println!("cargo:rustc-link-arg=-Wl,--enable-stdcall-fixup");
            },
            _ => {}
        }
        
        println!("cargo:rustc-link-arg=-Wl,--as-needed");
        println!("cargo:rustc-link-arg=-Wl,--gc-sections");
        
        if target_os == "linux" {
            println!("cargo:rustc-link-arg=-Wl,-z,relro");
            println!("cargo:rustc-link-arg=-Wl,-z,now");
        } else if target_os == "windows" {
            println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
            println!("cargo:rustc-link-arg=-Wl,--dynamicbase");
            println!("cargo:rustc-link-arg=-Wl,--nxcompat");
        }
    }
    
    println!("cargo:rustc-cfg=feature=\"shabal_base\"");
    
    if target_arch == "x86_64" {
        println!("cargo:rustc-cfg=feature=\"shabal_sse2\"");
        println!("cargo:rustc-cfg=feature=\"shabal_avx\"");
        println!("cargo:rustc-cfg=feature=\"shabal_avx2\"");
        
        if target_os != "macos" && std::path::Path::new("src/c/shabal_avx512f.c").exists() {
            println!("cargo:rustc-cfg=feature=\"shabal_avx512f\"");
        }
    } else if target_arch == "aarch64" || target_arch == "arm" {
        if std::path::Path::new("src/c/shabal_neon.c").exists() {
            println!("cargo:rustc-cfg=feature=\"shabal_neon\"");
        }
    }
    
    if target_os == "macos" {
        let deployment_target = if target_arch == "aarch64" { "11.0" } else { "10.12" };
        println!("cargo:rustc-link-arg=-mmacosx-version-min={}", deployment_target);
    }

    println!("cargo:rerun-if-changed=src/c/");
    println!("cargo:rerun-if-changed=build.rs");
}