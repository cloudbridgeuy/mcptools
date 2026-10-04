use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn describe(base: &str) -> String {
    let Some(described) = git(&["describe", "--tags", "--long", "--dirty", "--match", "v*"]) else {
        return base.to_string();
    };
    let (rest, dirty) = match described.strip_suffix("-dirty") {
        Some(rest) => (rest, true),
        None => (described.as_str(), false),
    };
    let mut parts = rest.rsplitn(3, '-');
    let (Some(hash), Some(count)) = (parts.next(), parts.next()) else {
        return base.to_string();
    };
    if count == "0" && !dirty {
        return base.to_string();
    }
    let suffix = if dirty { ".dirty" } else { "" };
    format!("{base}+{count}.{hash}{suffix}")
}

fn main() {
    for path in ["HEAD", "index", "refs", "packed-refs"] {
        if let Some(p) = git(&["rev-parse", "--git-path", path]) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    let base = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    println!("cargo:rustc-env=MCPTOOLS_VERSION={}", describe(&base));
}
