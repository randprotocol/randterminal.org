fn main() {
    let rev = std::process::Command::new("git").args(["-C", "../../../circuits", "rev-parse", "--short=7", "HEAD"]).output()
        .ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=TERMINAL_CIRCUITS_REV={rev}");
    println!("cargo:rerun-if-changed=../../../.git/modules/circuits/HEAD");
}
