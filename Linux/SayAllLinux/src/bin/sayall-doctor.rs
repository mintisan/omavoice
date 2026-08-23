use sayall_linux::i18n::{Language, language, tr};
use sayall_linux::{
    CliAction, USAGE_EXIT_CODE, collect_system_snapshot, evaluate, exit_code, help_text,
    parse_args, render_human, render_json,
};
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let action = match parse_args(env::args().skip(1)) {
        Ok(action) => action,
        Err(error) => {
            eprintln!("{}: {error}\n\n{}", tr("Error", "错误"), help_text());
            return ExitCode::from(USAGE_EXIT_CODE as u8);
        }
    };

    let CliAction::Run(options) = action else {
        print!("{}", help_text());
        return ExitCode::SUCCESS;
    };

    let report = evaluate(&collect_system_snapshot(), options.phase);
    if options.json {
        match render_json(&report) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                match language() {
                    Language::English => eprintln!("Error: could not generate JSON: {error}"),
                    Language::SimplifiedChinese => eprintln!("错误：无法生成 JSON：{error}"),
                }
                return ExitCode::FAILURE;
            }
        }
    } else {
        print!("{}", render_human(&report));
    }

    ExitCode::from(exit_code(&report) as u8)
}
