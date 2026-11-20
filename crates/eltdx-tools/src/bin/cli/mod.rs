mod args;
mod extension;
mod f10;
mod hq;
mod offline;
mod output;

use std::ffi::OsString;
use std::io::{self, Write};
use std::process::ExitCode;
use std::time::Duration;

use clap::{CommandFactory, Parser};
use eltdx::{ErrorKind, OperationOptions, RawPolicy, TradingDate};

use args::{Cli, Commands};
use output::object;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn run(command: Commands) -> Result<()> {
    let (options, payload) = match command {
        Commands::Version(output) => {
            let payload = object! {
                "version" => env!("CARGO_PKG_VERSION"),
                "python" => Option::<&str>::None,
            };
            (output, payload)
        }
        Commands::Ping(args) => (args.output.clone(), hq::ping(args)?),
        Commands::Workday(args) => (args.common.output.clone(), hq::workday(args)?),
        Commands::Quote(args) => (args.common.output.clone(), hq::quote(args)?),
        Commands::Kline(args) => (args.common.output.clone(), hq::kline(args)?),
        Commands::Minute(args) => (args.common.output.clone(), hq::minute(args)?),
        Commands::Trades(args) => (args.common.output.clone(), hq::trades(args)?),
        Commands::Codes(args) => (args.common.output.clone(), hq::directory(args)?),
        Commands::Finance(args) => (args.common.output.clone(), hq::finance(args)?),
        Commands::MoneyFlow(args) => (args.common.output.clone(), hq::money_flow(args)?),
        Commands::Limits(args) => (args.common.output.clone(), hq::limits(args)?),
        Commands::Ex(args) => (args.common.output.clone(), extension::ex(args)?),
        Commands::Mac(args) => (args.common.output.clone(), extension::mac(args)?),
        Commands::Macex(args) => (args.common.output.clone(), extension::macex(args)?),
        Commands::Fund(args) => (args.common.output.clone(), extension::fund(args)?),
        Commands::F10(args) => (args.common.output.clone(), output::scalar(f10::run(args)?)),
        Commands::Index(args) => (
            args.common.output.clone(),
            output::scalar(f10::index(args)?),
        ),
        Commands::Ipo(args) => (args.output.clone(), output::scalar(f10::ipo(args)?)),
        Commands::Offline(args) => (args.output.clone(), offline::run(args)?),
    };
    let mut stdout = io::stdout().lock();
    output::emit(&mut stdout, &options, payload)?;
    stdout.flush()?;
    Ok(())
}

pub fn main() -> ExitCode {
    let argv: Vec<_> = std::env::args_os().collect();
    let parsed = Cli::try_parse_from(&argv).or_else(|original| {
        let Some(reordered) = reorder_codes(&argv) else {
            return Err(original);
        };
        Cli::try_parse_from(reordered).map_err(|_| original)
    });
    let result = match parsed {
        Ok(Cli {
            command: Some(command),
        }) => run(command),
        Ok(Cli { command: None }) => Cli::command().print_help().map_err(Into::into),
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            return ExitCode::from(if code == 0 { 0 } else { 2 });
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("错误: {error}");
            let cancelled = error
                .downcast_ref::<eltdx::Error>()
                .is_some_and(|sdk| sdk.kind() == ErrorKind::Cancelled);
            ExitCode::from(if cancelled { 130 } else { 1 })
        }
    }
}

fn reorder_codes(argv: &[OsString]) -> Option<Vec<OsString>> {
    let parser = Cli::command();
    let command = parser.find_subcommand(argv.get(1)?.to_str()?)?;
    if !command
        .get_arguments()
        .any(|argument| argument.get_id() == "code" && argument.is_positional())
    {
        return None;
    }
    let mut options = Vec::new();
    let mut positionals = Vec::new();
    let mut tokens = argv[2..].iter();
    let mut after_separator = false;
    while let Some(token) = tokens.next() {
        let text = token.to_str()?;
        if text == "--" {
            positionals.extend(tokens.cloned());
            after_separator = true;
            break;
        }
        if !text.starts_with('-') || text == "-" {
            positionals.push(token.clone());
            continue;
        }
        let takes_separate_value = if let Some(long) = text.strip_prefix("--") {
            let (name, attached) = long
                .split_once('=')
                .map_or((long, false), |(name, _)| (name, true));
            let argument = command
                .get_arguments()
                .find(|argument| argument.get_long() == Some(name))?;
            argument.get_action().takes_values() && !attached
        } else {
            if !text.starts_with("-o") || text.starts_with("-output") {
                return None;
            }
            text == "-o"
        };
        options.push(token.clone());
        if takes_separate_value {
            options.push(tokens.next()?.clone());
        }
    }
    let mut reordered = argv[..2].to_vec();
    reordered.extend(options);
    if after_separator {
        reordered.push("--".into());
    }
    reordered.extend(positionals);
    Some(reordered)
}

fn codes(values: &[String]) -> Vec<&str> {
    values
        .iter()
        .flat_map(|value| value.split(|ch: char| ch == ',' || ch.is_whitespace()))
        .filter(|value| !value.is_empty())
        .collect()
}

fn single_code<'a>(values: &'a [&str]) -> Result<&'a str> {
    match values {
        [code] => Ok(code),
        _ => Err(invalid(format!(
            "该命令只接受一个代码，收到 {} 个: {values:?}",
            values.len()
        ))),
    }
}

fn invalid(message: impl std::fmt::Display) -> Box<dyn std::error::Error> {
    format!("ValueError: {message}").into()
}

fn required<T>(value: Option<T>) -> Result<T> {
    value.ok_or_else(|| invalid("缺少必需参数（见 --help）"))
}

fn trade_date(value: Option<&str>) -> Result<Option<TradingDate>> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let digits = match value.trim().as_bytes() {
        [a, b, c, d, e, f, g, h] => [*a, *b, *c, *d, *e, *f, *g, *h],
        [a, b, c, d, b'-', e, f, b'-', g, h] => [*a, *b, *c, *d, *e, *f, *g, *h],
        _ => {
            return Err(invalid(format!(
                "日期格式应为 YYYYMMDD 或 YYYY-MM-DD，收到 {value:?}"
            )))
        }
    };
    if !digits.iter().all(u8::is_ascii_digit) {
        return Err(invalid(format!(
            "日期格式应为 YYYYMMDD 或 YYYY-MM-DD，收到 {value:?}"
        )));
    }
    let number = |part: &[u8]| {
        part.iter()
            .fold(0_u16, |acc, digit| acc * 10 + u16::from(digit - b'0'))
    };
    TradingDate::new(
        number(&digits[..4]),
        number(&digits[4..6]) as u8,
        number(&digits[6..]) as u8,
    )
    .map(Some)
    .map_err(|err| invalid(err))
}

fn timeout(seconds: f64) -> Result<Duration> {
    Duration::try_from_secs_f64(seconds)
        .ok()
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| invalid("timeout must be positive and finite"))
}

fn operation(timeout: Duration, raw: bool) -> OperationOptions {
    OperationOptions::default()
        .with_timeout(timeout)
        .with_raw(if raw {
            RawPolicy::RetainPayload
        } else {
            RawPolicy::Omit
        })
}

fn finish<T>(result: Result<T>, close: eltdx::Result<()>) -> Result<T> {
    let payload = result?;
    close?;
    Ok(payload)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;

    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
