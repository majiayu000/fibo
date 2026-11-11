//! `fibo` in a development build: runs the `mooring` program beside it with the same
//! arguments. The app bundle has a link by this name instead (package-macos.py), so a
//! terminal of either kind can type the command by its name.
#[macro_use]
extern crate mooring_core;

use std::process::Command;

fn main() {
    mooring_core::legacy::adopt_old_env();
    if let Some(choice) = std::env::var("MOORING_LANG")
        .ok()
        .and_then(|v| mooring_core::i18n::Choice::from_key(&v))
    {
        mooring_core::i18n::set(choice.resolve(&mooring_platform::desktop::preferred_languages()));
    }
    let Some(program) = std::env::current_exe().ok().map(|exe| {
        exe.with_file_name("mooring")
            .with_extension(std::env::consts::EXE_EXTENSION)
    }) else {
        eprintln!("fibo: {}", t!("找不到自己在哪"));
        std::process::exit(1);
    };
    let mut command = Command::new(&program);
    command.args(std::env::args_os().skip(1));
    let error = mooring_platform::process::become_command(&mut command);
    eprintln!(
        "fibo: {}",
        t!(
            "运行 {program} 失败：{error}",
            program = program.display(),
            error
        )
    );
    std::process::exit(1);
}
