use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    for name in [
        "MAGEKIT_UV_BUNDLE",
        "MAGEKIT_SKIP_UV_BUNDLE",
        "MAGEKIT_BUILD_PYTHON",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    println!("cargo:rerun-if-changed=build_support/bundle_uv.py");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let destination = out.join("magekit-uv.bin");
    if let Some(source) = env::var_os("MAGEKIT_UV_BUNDLE") {
        let source = PathBuf::from(source)
            .canonicalize()
            .expect("MAGEKIT_UV_BUNDLE must exist");
        println!("cargo:rerun-if-changed={}", source.display());
        assert!(
            fs::metadata(&source).unwrap().len() > 0,
            "uv bundle must not be empty"
        );
        fs::copy(source, destination).expect("copy uv bundle");
    } else if env::var("PROFILE").as_deref() == Ok("release")
        && env::var("MAGEKIT_SKIP_UV_BUNDLE").as_deref() != Ok("1")
    {
        // Runs on the build HOST, downloads for TARGET (important for macOS universal builds).
        let script = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("build_support/bundle_uv.py");
        let candidates = env::var("MAGEKIT_BUILD_PYTHON")
            .map(|p| vec![p])
            .unwrap_or_else(|_| {
                if cfg!(windows) {
                    vec!["python".into(), "python3".into()]
                } else {
                    vec!["python3".into(), "python".into()]
                }
            });
        let mut success = false;
        for python in candidates {
            match Command::new(python)
                .arg(&script)
                .arg(env::var("TARGET").unwrap())
                .arg(&destination)
                .status()
            {
                Ok(status) => {
                    success = status.success();
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => panic!("cannot start uv bundler: {e}"),
            }
        }
        assert!(
            success,
            "uv bundling failed. See docs/streamlink.md; supply MAGEKIT_UV_BUNDLE for offline builds."
        );
    } else {
        fs::write(destination, []).expect("write empty development bundle");
        println!(
            "cargo:warning=uv is not embedded: use MAGEKIT_UV_PATH or uv on PATH for development"
        );
    }
}
