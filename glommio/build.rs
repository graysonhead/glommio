use std::{env, fs, path::*, process::Command};

use cc::Build;
use pkg_config::Config;

fn main() {
    let project = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .canonicalize()
        .unwrap();

    if let Ok(lib) = Config::new().probe("liburing") {
        // Compile our bindings against the system liburing, if found
        Build::new()
            .file(project.join("rusturing.c"))
            .flag("-D_GNU_SOURCE")
            .includes(&lib.include_paths)
            .compile("rusturing");
        return;
    }

    // // First check if liburing can be found via pkg-config
    // let use_system_liburing = match Config::new().probe("liburing") {
    //     Ok(lib) => {
    //         println!("Found liburing via pkg-config");

    //         // Only compile our bindings against the system library
    //         Build::new()
    //             .file(project.join("rusturing.c"))
    //             .flag("-D_GNU_SOURCE")
    //             .includes(&lib.include_paths)
    //             .compile("rusturing");

    //         true
    //     }
    //     Err(_) => {
    //         // If both env vars are set, use them instead of building from source
    //         if let (Ok(lib_dir), Ok(include_dir)) = (
    //             env::var("GLOMMIO_LIBURING_LIB"),
    //             env::var("GLOMMIO_LIBURING_INCLUDE"),
    //         ) {
    //             println!("Using liburing from environment variables");
    //             println!("cargo:rustc-link-search={}", lib_dir);
    //             println!("cargo:rustc-link-lib=uring");

    //             Build::new()
    //                 .file(project.join("rusturing.c"))
    //                 .flag("-D_GNU_SOURCE")
    //                 .include(&include_dir)
    //                 .compile("rusturing");

    //             true
    //         } else {
    //             false
    //         }
    //     }
    // };

    // Fall back to building from source
    let liburing = match env::var("GLOMMIO_LIBURING_DIR") {
        Ok(path) => {
            let path_buf = PathBuf::from(path);
            if path_buf.try_exists().unwrap_or(false) {
                path_buf
            } else {
                eprintln!("Warning: GLOMMIO_LIBURING_DIR path does not exist, falling back to git");
                project.join("liburing")
            }
        }
        Err(_) => {
            // Try to initialize the git submodule
            let status = Command::new("git")
                .arg("submodule")
                .arg("update")
                .arg("--init")
                .status();

            if status.is_err() {
                eprintln!("Warning: Failed to initialize git submodule");
            }

            project.join("liburing")
        }
    };

    // Check if liburing directory exists before proceeding
    if !liburing.try_exists().unwrap_or(false) {
        panic!("Cannot find liburing source directory at {:?}", liburing);
    }

    // Run the configure script in OUT_DIR to get `compat.h`
    let configured_include = configure(&liburing);

    let src = liburing.join("src");

    // liburing
    Build::new()
        .file(src.join("setup.c"))
        .file(src.join("queue.c"))
        .file(src.join("syscall.c"))
        .file(src.join("register.c"))
        .flag("-D_GNU_SOURCE")
        .include(src.join("include"))
        .include(&configured_include)
        .extra_warnings(false)
        .compile("uring");

    // (our additional, linkable C bindings)
    Build::new()
        .file(project.join("rusturing.c"))
        .flag("-D_GNU_SOURCE")
        .include(src.join("include"))
        .include(&configured_include)
        .compile("rusturing");
}

fn configure(liburing: &Path) -> PathBuf {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap())
        .canonicalize()
        .unwrap();

    // Check if configure script exists
    let configure_path = liburing.join("configure");
    if !configure_path.try_exists().unwrap_or(false) {
        panic!("Configure script not found at {:?}", configure_path);
    }

    fs::create_dir_all(&out_dir).unwrap_or_else(|e| {
        panic!("Failed to create out_dir: {}", e);
    });

    fs::copy(&configure_path, out_dir.join("configure")).unwrap_or_else(|e| {
        panic!("Failed to copy configure script: {}", e);
    });

    fs::create_dir_all(out_dir.join("src/include/liburing")).unwrap_or_else(|e| {
        panic!("Failed to create include directory: {}", e);
    });

    let output = Command::new("./configure").current_dir(&out_dir).output();

    if let Err(e) = output {
        panic!("Failed to run configure script: {}", e);
    }

    out_dir.join("src/include")
}
