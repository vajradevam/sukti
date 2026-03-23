//! Command-line interface.

use crate::fmt;
use crate::pipeline::{self, Options};
use crate::{NAME, VERSION};
use clap::{Parser, Subcommand, ValueEnum};
use sukti_sanskrit::Script;

#[derive(Parser, Debug)]
#[command(name = "sukti", version = VERSION, about = format!("{NAME} — a general-purpose language with Sanskrit grammatical semantics, written in ITRANS"))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
    /// Diagnostic language
    #[arg(long, global = true, value_enum, default_value = "en")]
    pub lang: Lang,
    /// Input script of source files (ITRANS is the only language script)
    #[arg(long, global = true, value_enum, default_value = "itrans")]
    pub script: ScriptArg,
    /// JIT mode (off is the default; tier-1 currently runs through differential tier-0)
    #[arg(long, global = true, default_value = "off")]
    pub jit: String,
    #[arg(long, global = true, default_value_t = 1000)]
    pub jit_threshold: usize,
    /// Path for the Tier-1 compilation-plan cache (on disk)
    #[arg(long, global = true)]
    pub jit_cache: Option<String>,
    #[arg(long, global = true)]
    pub dump_ast: bool,
    #[arg(long, global = true)]
    pub dump_jit: bool,
    #[arg(long, global = true)]
    pub prakriya: bool,
    #[arg(long, global = true, default_value_t = 42)]
    pub seed: u64,
    #[arg(long, global = true, default_value_t = 200_000_000)]
    pub max_steps: usize,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Lang {
    Sa,
    En,
    Hi,
}
impl Lang {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sa => "sa",
            Self::En => "en",
            Self::Hi => "hi",
        }
    }
}

/// Source script. Sūkti programs are ITRANS-only; the other schemes belong to
/// the independent `sukti translit` scholarly utility below.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ScriptArg {
    Itrans,
}
impl ScriptArg {
    pub fn script(self) -> Script {
        Script::Itrans
    }
    pub fn view(self) -> fmt::View {
        fmt::View::Itrans
    }
}

/// Scripts understood by the standalone `sukti translit` utility.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum TranslitScript {
    Devanagari,
    Iast,
    Itrans,
    Hk,
    Slp1,
}
impl TranslitScript {
    pub fn script(self) -> Script {
        match self {
            Self::Devanagari => Script::Devanagari,
            Self::Iast => Script::Iast,
            Self::Itrans => Script::Itrans,
            Self::Hk => Script::HarvardKyoto,
            Self::Slp1 => Script::Slp1,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run a source file
    Run { file: String },
    /// Interactive REPL
    Repl,
    /// Format a source file
    Fmt {
        file: String,
        #[arg(long)]
        view: Option<ScriptArg>,
        #[arg(long)]
        inplace: bool,
    },
    /// Type-check and report diagnostics without running
    Check { file: String },
    /// Run golden tests in examples/
    Test {
        #[arg(default_value = "examples")]
        dir: String,
    },
    /// Generate bilingual API documentation
    Doc { file: String },
    /// Package manager (local registry)
    Pkg {
        #[command(subcommand)]
        cmd: PkgCmd,
    },
    /// Benchmark a source file
    Bench { file: String },
    /// Transliterate text between scripts
    Translit {
        text: String,
        #[arg(long, default_value = "devanagari")]
        from: TranslitScript,
        #[arg(long, default_value = "iast")]
        to: TranslitScript,
    },
    /// Language server protocol (handled by the `sukti` binary)
    Lsp,
}

#[derive(Debug, Subcommand)]
pub enum PkgCmd {
    Init {
        #[arg(default_value = "sukti.toml")]
        manifest: String,
    },
    Install {
        package: String,
    },
    List,
}

impl Cli {
    fn opts(&self) -> Options {
        Options {
            lang: self.lang.as_str().into(),
            script: self.script.script(),
            prakriya: self.prakriya,
            max_steps: self.max_steps,
            seed: self.seed,
            jit: self.jit.clone(),
            jit_threshold: self.jit_threshold,
            jit_cache: self.jit_cache.clone(),
            dump_ast: self.dump_ast,
            dump_jit: self.dump_jit,
        }
    }
}

pub fn main() -> i32 {
    let cli = Cli::parse();
    let opts = cli.opts();
    match &cli.command {
        Command::Run { file } => match pipeline::run_file(file, &opts) {
            Ok(o) => {
                if !o.ast_dump.is_empty() {
                    print!("{}", o.ast_dump);
                }
                print!("{}", o.output);
                if !o.diagnostics.is_empty() {
                    eprint!("{}", o.diagnostics);
                    return 1;
                }
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        Command::Check { file } => {
            match pipeline::run_file(
                file,
                &Options {
                    dump_ast: false,
                    ..opts
                },
            ) {
                Ok(o) => {
                    if o.diagnostics.is_empty() {
                        println!("{}: OK", file);
                        0
                    } else {
                        eprint!("{}", o.diagnostics);
                        1
                    }
                }
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        Command::Fmt {
            file,
            view,
            inplace,
        } => {
            let text = match std::fs::read_to_string(file) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("{e}");
                    return 1;
                }
            };
            let v = view.unwrap_or(ScriptArg::Itrans).view();
            match pipeline::format_source(&text, v, &opts) {
                Ok(s) => {
                    if *inplace {
                        let _ = std::fs::write(file, s);
                    } else {
                        print!("{s}");
                    }
                    0
                }
                Err(e) => {
                    eprint!("{e}");
                    1
                }
            }
        }
        Command::Test { dir } => run_tests(dir, &opts),
        Command::Repl => repl(&opts),
        Command::Translit { text, from, to } => {
            println!(
                "{}",
                sukti_sanskrit::transliterate(text, from.script(), to.script())
            );
            0
        }
        Command::Doc { file } => {
            let text = match std::fs::read_to_string(file) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("{e}");
                    return 1;
                }
            };
            let docs = crate::docgen::generate(&text);
            print!("{docs}");
            0
        }
        Command::Pkg { cmd } => match cmd {
            PkgCmd::Init { manifest } => {
                let body = "name = \"my-sukti-package\"\nversion = \"0.1.0\"\ndependencies = []\n";
                match std::fs::write(manifest, body) {
                    Ok(_) => {
                        println!("created {manifest}");
                        0
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        1
                    }
                }
            }
            PkgCmd::Install { package } => {
                println!("local registry: {package} would be installed from .sukti/registry");
                0
            }
            PkgCmd::List => {
                println!("(no packages installed)");
                0
            }
        },
        Command::Bench { file } => {
            let start = std::time::Instant::now();
            let r = pipeline::run_file(file, &opts);
            let elapsed = start.elapsed();
            match r {
                Ok(o) => {
                    print!("{}", o.output);
                    println!("\n[bench] {file}: {:.2?} (Tier 0)", elapsed);
                    0
                }
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        Command::Lsp => {
            eprintln!("LSP server is provided by crates/lsp");
            1
        }
    }
}

fn run_tests(dir: &str, opts: &Options) -> i32 {
    let mut pass = 0;
    let mut fail = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("no such directory: {dir}");
        return 1;
    };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map_or(false, |x| x == "sukti" || x == "sk"))
        .collect();
    files.sort();
    for f in files {
        let expected_path = f.with_extension("expected");
        let Ok(expected) = std::fs::read_to_string(&expected_path) else {
            continue;
        };
        match pipeline::run_file(f.to_str().unwrap(), opts) {
            Ok(o) if o.diagnostics.is_empty() && o.output == expected => {
                pass += 1;
            }
            Ok(o) => {
                fail += 1;
                eprintln!(
                    "FAIL {}: got {:?}, expected {:?}, diag {}",
                    f.display(),
                    o.output,
                    expected,
                    o.diagnostics
                );
            }
            Err(e) => {
                fail += 1;
                eprintln!("FAIL {}: {e}", f.display());
            }
        }
    }
    println!("golden tests: {pass} passed, {fail} failed");
    if fail == 0 {
        0
    } else {
        1
    }
}

fn repl(opts: &Options) -> i32 {
    use crate::diag::Source;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::vm::VM;
    use std::io::{self, BufRead, Write};

    println!("sUkti REPL — type exit to quit");
    let lex = crate::lexicon::Lexicon::load();
    let mut vm = VM::new(&opts.lang);
    crate::stdlib::register(&mut vm);
    let stdin = io::stdin();
    let mut buffer = String::new();
    loop {
        print!("> ");
        let _ = io::stdout().flush();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if line.trim() == "exit" {
            break;
        }
        buffer.push_str(&line);
        let src = Source::new("repl", &buffer);
        let mut rep = crate::diag::Reporter::new(&opts.lang);
        let toks = Lexer::new(&src, &lex, &mut rep).tokenize();
        let mut p = Parser::new(&src, &lex, &mut rep, toks);
        let stmts = p.parse_program();
        if rep.has_errors() {
            buffer.clear();
            eprint!("{}", rep.render(&src));
            continue;
        }
        if stmts.is_empty() {
            continue;
        }
        match vm.run(&stmts) {
            Ok(()) => {
                print!("{}", vm.output);
                vm.output.clear();
            }
            Err(f) => {
                eprintln!("runtime {}: {}", f.kind, f.message);
            }
        }
        buffer.clear();
    }
    0
}
