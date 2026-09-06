use ctab_web_companion::{CompanionOptions, start};
use std::path::PathBuf;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let options = match parse_options(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("cTab Web Companion startup error: {message}");
            return ExitCode::from(2);
        }
    };

    match start(options).await {
        Ok(runtime) => match runtime.wait().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("cTab Web Companion runtime error: {error}");
                ExitCode::from(1)
            }
        },
        Err(error) => {
            eprintln!("cTab Web Companion startup error: {error}");
            ExitCode::from(1)
        }
    }
}

fn parse_options(arguments: impl Iterator<Item = String>) -> Result<CompanionOptions, String> {
    let mut pipe_name = None;
    let mut pipe_token = None;
    let mut open_browser = true;
    let mut arma_root = None;
    let mut arguments = arguments.peekable();

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--pipe-name" => pipe_name = arguments.next(),
            "--pipe-token" => pipe_token = arguments.next(),
            "--no-browser" => open_browser = false,
            "--arma-root" => arma_root = arguments.next().map(PathBuf::from),
            _ => return Err("unsupported command-line option".to_owned()),
        }
    }

    Ok(CompanionOptions {
        pipe_name: pipe_name.ok_or_else(|| "missing --pipe-name".to_owned())?,
        pipe_token: pipe_token.ok_or_else(|| "missing --pipe-token".to_owned())?,
        open_browser,
        arma_root,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_fixed_startup_contract() {
        let options = parse_options(
            [
                "--pipe-name",
                r"\\.\pipe\ctab-web-test",
                "--pipe-token",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "--no-browser",
                "--arma-root",
                r"X:\Games\Arma 3",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .expect("valid options");
        assert!(!options.open_browser);
        assert_eq!(options.arma_root, Some(PathBuf::from(r"X:\Games\Arma 3")));
    }

    #[test]
    fn rejects_unknown_options() {
        assert!(parse_options(["--listen-all".to_owned()].into_iter()).is_err());
    }
}
