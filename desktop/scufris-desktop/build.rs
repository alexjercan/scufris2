use std::{fs, io, path::Path, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=ui/pill.ts");
    println!("cargo:rerun-if-changed=ui/pill.css");
    println!("cargo:rerun-if-changed=ui/index.html");
    println!("cargo:rerun-if-changed=ui/review.ts");
    println!("cargo:rerun-if-changed=ui/review.css");
    println!("cargo:rerun-if-changed=ui/review.html");
    println!("cargo:rerun-if-changed=ui/orb-engine.js");
    println!("cargo:rerun-if-changed=ui/orb-engine.d.ts");
    println!("cargo:rerun-if-changed=ui/tauri.d.ts");
    println!("cargo:rerun-if-changed=ui/tsconfig.json");
    build_frontend();
    tauri_build::build();
}

/// Compiles the frontend scripts and assembles the complete frontend in
/// ui/dist, the directory tauri.conf.json embeds. A frontend that does not
/// compile fails the build; there is no shipping the pill without its script.
fn build_frontend() {
    let ui = Path::new("ui");
    let status = tsc(&ui.join("tsconfig.json"))
        .expect("tsc is required to build the pill frontend and was not found");
    assert!(status.success(), "tsc failed on the pill frontend");
    for file in [
        "index.html",
        "pill.css",
        "review.html",
        "review.css",
        "orb-engine.js",
    ] {
        fs::copy(ui.join(file), ui.join("dist").join(file))
            .unwrap_or_else(|error| panic!("could not copy ui/{file} into ui/dist: {error}"));
    }
}

/// Runs the compiler from PATH, or through npx from the repository checkout
/// when the shell does not carry tsc itself.
fn tsc(project: &Path) -> io::Result<std::process::ExitStatus> {
    match Command::new("tsc").arg("-p").arg(project).status() {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Command::new("npx")
            .args(["--no-install", "tsc", "-p"])
            .arg(project)
            .status(),
        outcome => outcome,
    }
}
