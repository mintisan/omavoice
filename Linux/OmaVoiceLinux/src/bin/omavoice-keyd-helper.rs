use omavoice_linux::i18n::{Language, language, tr};
use omavoice_linux::keyd_apply::{ApplyOutcome, apply_rc003_keyd_config};
use std::io::{self, Read};

const MAX_CONFIG_BYTES: u64 = 16 * 1024;
const USAGE_EXIT_CODE: i32 = 64;

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.as_slice() != ["apply-v1"] {
        eprintln!(
            "{}",
            tr(
                "Usage: omavoice-keyd-helper apply-v1 (reads an OmaVoice keyd configuration from standard input)",
                "用法：omavoice-keyd-helper apply-v1（从标准输入读取 OmaVoice keyd 配置）"
            )
        );
        std::process::exit(USAGE_EXIT_CODE);
    }

    let mut bytes = Vec::new();
    if let Err(error) = io::stdin()
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
    {
        match language() {
            Language::English => eprintln!("Failed to read keyd configuration: {error}"),
            Language::SimplifiedChinese => eprintln!("读取 keyd 配置失败：{error}"),
        }
        std::process::exit(1);
    }
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        match language() {
            Language::English => {
                eprintln!("Refusing a keyd configuration larger than {MAX_CONFIG_BYTES} bytes")
            }
            Language::SimplifiedChinese => {
                eprintln!("拒绝超过 {MAX_CONFIG_BYTES} bytes 的 keyd 配置")
            }
        }
        std::process::exit(1);
    }
    let config = match std::str::from_utf8(&bytes) {
        Ok(config) => config,
        Err(error) => {
            match language() {
                Language::English => {
                    eprintln!("The keyd configuration is not valid UTF-8: {error}")
                }
                Language::SimplifiedChinese => eprintln!("keyd 配置不是有效 UTF-8：{error}"),
            }
            std::process::exit(1);
        }
    };

    match apply_rc003_keyd_config(config) {
        Ok(ApplyOutcome::Applied) => println!(
            "{}",
            tr(
                "Validated, backed up, and applied the RC003 button configuration",
                "已校验、备份并应用 RC003 按键配置"
            )
        ),
        Ok(ApplyOutcome::Unchanged) => println!(
            "{}",
            tr(
                "The RC003 system button configuration is already up to date",
                "RC003 系统按键配置已经是最新版本"
            )
        ),
        Err(error) => {
            match language() {
                Language::English => {
                    eprintln!("Failed to apply the RC003 button configuration: {error}")
                }
                Language::SimplifiedChinese => eprintln!("应用 RC003 按键配置失败：{error}"),
            }
            std::process::exit(1);
        }
    }
}
