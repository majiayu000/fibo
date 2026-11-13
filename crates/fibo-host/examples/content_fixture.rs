//! Isolated native acceptance daemon: only watches fixture logs, never user CLI directories.
fn main() -> std::io::Result<()> {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("fixture runtime"));
    std::fs::create_dir_all(root.join("logs"))?;
    fibo_host::serve_with_roots(
        &root,
        vec![
            (fibo_core::model::Agent::Codex, root.join("logs")),
            (fibo_core::model::Agent::Claude, root.join("claude-logs")),
        ],
    )
}
