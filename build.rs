use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    // Track the active branch ref file so commit updates trigger rebuilds
    if let Ok(head_content) = fs::read_to_string(".git/HEAD") {
        if let Some(ref_path) = head_content.strip_prefix("ref: ") {
            let branch_ref = ref_path.trim();
            let branch_file = format!(".git/{}", branch_ref);
            if Path::new(&branch_file).exists() {
                println!("cargo:rerun-if-changed={}", branch_file);
            }
        }
    }
    if Path::new(".git/packed-refs").exists() {
        println!("cargo:rerun-if-changed=.git/packed-refs");
    }

    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();

    let git_hash = match output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8(out.stdout).unwrap_or_else(|_| "unknown".to_string());
            let trimmed = s.trim();
            if trimmed.is_empty() {
                "unknown".to_string()
            } else {
                trimmed.to_string()
            }
        }
        _ => "unknown".to_string(),
    };

    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=FIRMWARE_VERSION=v{}", env!("CARGO_PKG_VERSION"));
}

