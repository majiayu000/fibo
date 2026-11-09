//! `fibo` in a development build: runs the `mooring` program beside it with the same
//! arguments. The app bundle has a link by this name instead (package-macos.py), so a
//! terminal of either kind can type the command by its name.
use std::process::Command;

fn main() {
    let Some(program) = std::env::current_exe().ok().map(|exe| {
        exe.with_file_name("mooring")
            .with_extension(std::env::consts::EXE_EXTENSION)
    }) else {
        eprintln!("fibo: 找不到自己在哪");
        std::process::exit(1);
    };
    let mut command = Command::new(&program);
    command.args(std::env::args_os().skip(1));
    let error = mooring_platform::process::become_command(&mut command);
    eprintln!("fibo: 运行 {} 失败：{error}", program.display());
    std::process::exit(1);
}
