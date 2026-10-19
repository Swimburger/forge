//! The `forge` command, which generates an SDK from an OpenAPI document.
//!
//! ```text
//! forge openapi.json --target ts --out sdk
//! forge openapi.json --target wasm://plugin.wasm --out sdk
//! forge mcp openapi.json
//! ```
//!
//! `forge mcp`, which serves the methods of the document as the tools of an
//! MCP server, needs the `mcp` feature.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use forge::{Err, Forge};

#[cfg(feature = "mcp")]
mod mcp;
mod ts;

// The document, --target and --out are required unless a command is given,
// and are `Option`s so that a command can leave them out.
/// Generates an SDK from an OpenAPI document.
#[derive(Parser)]
#[command(version, args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
struct Args {
    #[cfg(feature = "mcp")]
    #[command(subcommand)]
    command: Option<Command>,
    /// The OpenAPI JSON document, or - to read it from standard input.
    #[arg(required = true)]
    spec: Option<PathBuf>,
    /// The language of the SDK: `ts` for TypeScript, or `wasm://PATH` for
    /// the plugin compiled to a WebAssembly component at PATH.
    #[arg(long, value_parser = Target::parse, required = true)]
    target: Option<Target>,
    /// The directory to write the SDK to.
    #[arg(long, required = true)]
    out: Option<PathBuf>,
}

/// What forge can do besides generating an SDK.
#[cfg(feature = "mcp")]
#[derive(clap::Subcommand)]
enum Command {
    /// Serves the methods of an OpenAPI document as the tools of an MCP
    /// server.
    Mcp(mcp::Args),
}

/// What forge generates an SDK with.
#[derive(Clone)]
enum Target {
    /// The TypeScript generator.
    Ts,
    /// A plugin compiled to a WebAssembly component, at the path.
    #[cfg(feature = "wasm")]
    Wasm(PathBuf),
}

impl Target {
    fn parse(s: &str) -> Result<Target, String> {
        if s == "ts" {
            return Ok(Target::Ts);
        }
        let Some(path) = s.strip_prefix("wasm://") else {
            return Err("possible values: ts, wasm://PATH".to_owned());
        };
        if path.is_empty() {
            return Err("wasm:// needs the path of a plugin, as in wasm://plugin.wasm".to_owned());
        }
        #[cfg(feature = "wasm")]
        return Ok(Target::Wasm(PathBuf::from(path)));
        #[cfg(not(feature = "wasm"))]
        Err("this forge was built without wasm plugins; build it with `--features wasm`".to_owned())
    }
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("forge: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the command `args` give, or else generates the SDK they ask for.
fn run(args: Args) -> Result<(), Err> {
    #[cfg(feature = "mcp")]
    if let Some(Command::Mcp(args)) = args.command {
        return mcp::run(args, read);
    }
    // Clap requires these when there is no command.
    let (Some(spec), Some(target), Some(out)) = (args.spec, args.target, args.out) else {
        unreachable!("clap requires the document, --target and --out");
    };
    match target {
        Target::Ts => {
            let forge = read(&spec)?;
            forge::write(&out, forge::run(&forge, &ts::Generator)?)
        }
        #[cfg(feature = "wasm")]
        Target::Wasm(path) => {
            // Compiling the plugin and reading the document are both slow,
            // so they run at the same time.
            let (plugin, forge) = rayon::join(|| forge::wasm::WasmPlugin::from_file(&path), || read(&spec));
            let (plugin, forge) = (plugin?, forge?);
            forge::write(&out, forge::run(&forge, &plugin)?)
        }
    }
}

/// Reads the document at `path`, or from standard input when `path` is `-`.
fn read(path: &Path) -> Result<Forge, Err> {
    let parse = || -> Result<Forge, Err> {
        let json = if path == Path::new("-") {
            let mut json = Vec::new();
            io::stdin().read_to_end(&mut json)?;
            json
        } else {
            fs::read(path)?
        };
        Ok(serde_json::from_slice(&json)?)
    };
    parse().map_err(|err| format!("{}: {err}", path.display()).into())
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Args;

    #[test]
    fn args() {
        Args::command().debug_assert();
    }
}
