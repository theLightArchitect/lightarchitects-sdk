//! Build script — tells cargo to recompile when the Svelte frontend dist changes.
fn main() {
    // Force recompilation when the Svelte frontend dist changes.
    // rust_embed bakes files at compile time but cargo has no way to detect
    // changes in the embedded folder without this directive.
    println!("cargo:rerun-if-changed=../lightarchitects-webshell-ui/dist/");
}
