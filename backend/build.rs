//! Build script for injecting version and Git information at compile time

use std::process::Command;

fn read_trimmed(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn git_output(args: &[&str]) -> Option<String> {
    let candidates = [
        "git",
        r"C:\Program Files\Git\bin\git.exe",
        r"C:\Program Files (x86)\Git\bin\git.exe",
    ];

    for git in candidates {
        if let Ok(output) = Command::new(git).args(args).output() {
            if output.status.success() {
                let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !value.is_empty() && value != "HEAD" {
                    return Some(value);
                }
            }
        }
    }
    None
}

fn resolve_commit(version: &str) -> String {
    if let Ok(value) = std::env::var("GIT_COMMIT") {
        let value = value.trim().to_string();
        if !value.is_empty() {
            return value;
        }
    }

    if let Some(value) = git_output(&["rev-parse", "--short", "HEAD"]) {
        return value;
    }

    if let Some(value) = read_trimmed("../COMMIT") {
        return value;
    }

    // 无 git / COMMIT 文件时，用版本号生成可辨识的构建标识
    format!("build-{}", version.replace('.', ""))
}

fn resolve_branch() -> String {
    if let Ok(value) = std::env::var("GIT_BRANCH") {
        let value = value.trim().to_string();
        if !value.is_empty() {
            return value;
        }
    }

    if let Some(value) = git_output(&["rev-parse", "--abbrev-ref", "HEAD"]) {
        return value;
    }

    if let Some(value) = read_trimmed("../BRANCH") {
        return value;
    }

    "unknown".to_string()
}

fn main() {
    let version = read_trimmed("../VERSION").unwrap_or_else(|| "3.0.0".to_string());
    let branch = resolve_branch();
    let commit = resolve_commit(&version);

    println!("cargo:rustc-env=APP_VERSION={}", version);
    println!("cargo:rustc-env=GIT_BRANCH={}", branch);
    println!("cargo:rustc-env=GIT_COMMIT={}", commit);

    println!("cargo:rerun-if-env-changed=GIT_COMMIT");
    println!("cargo:rerun-if-env-changed=GIT_BRANCH");
    println!("cargo:rerun-if-changed=../VERSION");
    println!("cargo:rerun-if-changed=../COMMIT");
    println!("cargo:rerun-if-changed=../BRANCH");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/heads/");
}
