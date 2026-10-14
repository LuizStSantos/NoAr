#[cfg(not(debug_assertions))]
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[cfg(not(debug_assertions))]
use build_print::info;

#[cfg(not(debug_assertions))]
const NPM_COMMAND: &str = "npm";

#[cfg(not(debug_assertions))]
const RUN_P_BIN: &str = "../../node_modules/.bin/run-p";

#[cfg(not(debug_assertions))]
fn run_npm(args: &[&str]) {
    let output = Command::new(NPM_COMMAND)
        .args(args)
        .current_dir("../..")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .and_then(|mut child| {
            let stdout = child.stdout.take().expect("Failed to capture stdout");
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let line = line?;
                info!("{}", line.trim());
            }
            child.wait_with_output()
        })
        .expect("Failed to execute command");

    if !output.status.success() {
        panic!("Command executed with failing error code");
    }
}

fn main() {
    #[cfg(not(debug_assertions))]
    {
        // These files live outside the Cargo package. Watch the frontend inputs
        // individually so changes rebuild the embedded assets without watching
        // frontend/dist, which this build script generates itself.
        for path in [
            "../../frontend/src",
            "../../frontend/public",
            "../../frontend/index.html",
            "../../frontend/env.d.ts",
            "../../frontend/tsconfig.app.json",
            "../../frontend/tsconfig.node.json",
            "../../tsconfig.json",
            "../../vite.config.ts",
            "../../package.json",
            "../../package-lock.json",
        ] {
            println!("cargo:rerun-if-changed={path}");
        }

        // The Docker image builds the frontend in its own stage and copies
        // frontend/dist in, so the Rust stage does not need Node.js.
        println!("cargo:rerun-if-env-changed=NOAR_FRONTEND_PREBUILT");
        let prebuilt = std::env::var("NOAR_FRONTEND_PREBUILT").as_deref() == Ok("1")
            && Path::new("../../frontend/dist/index.html").is_file();

        if prebuilt {
            info!("Using the prebuilt frontend in frontend/dist");
        } else {
            if !Path::new(RUN_P_BIN).exists() {
                info!("run-p not found, installing frontend dependencies with npm ci");
                run_npm(&["ci"]);
            }

            run_npm(&["run", "build"]);
        }
    }

    println!("cargo:rerun-if-env-changed=FFPLAYOUT_VAAPI_SHARED");
    if matches!(std::env::var("FFPLAYOUT_VAAPI_SHARED").as_deref(), Ok("1")) {
        println!("cargo:rustc-link-lib=dylib=va-drm");
        println!("cargo:rustc-link-lib=dylib=va");
        println!("cargo:rustc-link-lib=dylib=drm");
    }
}
