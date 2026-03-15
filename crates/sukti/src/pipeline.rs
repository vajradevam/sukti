//! Front-end + Tier-0 driver shared by the CLI, REPL and tests.

use crate::diag::{Reporter, Source};
use crate::fmt;
use crate::lexer::Lexer;
use crate::lexicon::Lexicon;
use crate::parser::Parser;
use crate::resolve::{self, Resolved};
use crate::vm::VM;
use sukti_sanskrit::Script;

#[derive(Clone, Debug)]
pub struct Options {
    pub lang: String,
    pub script: Script,
    pub prakriya: bool,
    pub max_steps: usize,
    pub seed: u64,
    pub jit: String,
    pub jit_threshold: usize,
    /// Optional path for the Tier-1 compilation-plan cache on disk.
    pub jit_cache: Option<String>,
    pub dump_ast: bool,
    pub dump_jit: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            lang: "en".into(),
            script: Script::Itrans,
            prakriya: false,
            max_steps: 200_000_000,
            seed: 42,
            jit: "off".into(),
            jit_threshold: 10,
            jit_cache: None,
            dump_ast: false,
            dump_jit: false,
        }
    }
}

pub struct Outcome {
    pub output: String,
    pub diagnostics: String,
    pub source: Source,
    pub resolved: Option<Resolved>,
    pub ast_dump: String,
}

/// Lex, parse, resolve and optionally execute a source string.
pub fn run_source(text: &str, name: &str, opts: &Options) -> Result<Outcome, String> {
    let canonical = fmt::to_canonical(text, opts.script);
    let src = Source::new(name, &canonical);
    let lex = Lexicon::load();
    let mut rep = Reporter::new(&opts.lang);
    let toks = Lexer::new(&src, &lex, &mut rep).tokenize();
    let mut parser = Parser::new(&src, &lex, &mut rep, toks);
    let parsed = parser.parse_program();
    if rep.has_errors() {
        return Ok(Outcome {
            output: String::new(),
            diagnostics: rep.render(&src),
            source: src,
            resolved: None,
            ast_dump: String::new(),
        });
    }
    let ast_dump = if opts.dump_ast {
        format!("{parsed:#?}")
    } else {
        String::new()
    };
    let resolved = resolve::resolve(parsed, &lex, &mut rep);
    let diagnostics = rep.render(&src);
    if rep.has_errors() {
        return Ok(Outcome {
            output: String::new(),
            diagnostics,
            source: src,
            resolved: None,
            ast_dump,
        });
    }
    let mut vm = VM::new(&opts.lang);
    crate::stdlib::register(&mut vm);
    vm.set_source(src.clone());
    vm.trace_prakriya = opts.prakriya;
    vm.max_steps = opts.max_steps;
    vm.rng_state = opts.seed;
    if opts.jit != "off" {
        crate::set_jit_cache_path(opts.jit_cache.clone());
        if let Some(factory) = crate::tier1_factory() {
            let threshold = if opts.jit == "aggressive" {
                1
            } else {
                opts.jit_threshold
            };
            vm.tier1 = Some(factory(threshold));
        }
    }
    if let Err(f) = vm.run(&resolved.stmts) {
        let msg = if let Some(sp) = f.span {
            format!(
                "{}: runtime {}: {}\n{}",
                name,
                f.kind,
                f.message,
                src.snippet(sp)
            )
        } else {
            format!("{}: runtime {}: {}", name, f.kind, f.message)
        };
        return Ok(Outcome {
            output: vm.output,
            diagnostics: msg,
            source: src,
            resolved: Some(resolved),
            ast_dump,
        });
    }
    let mut output = vm.output;
    if let Some(t) = &vm.tier1 {
        output.push_str(&t.stats());
        output.push('\n');
        if opts.dump_jit {
            output.push_str(&t.dump());
        }
    }
    Ok(Outcome {
        output,
        diagnostics,
        source: src,
        resolved: Some(resolved),
        ast_dump,
    })
}

/// Parse and format without executing.
pub fn format_source(text: &str, view: fmt::View, opts: &Options) -> Result<String, String> {
    let canonical = fmt::to_canonical(text, opts.script);
    let src = Source::new("input", &canonical);
    let lex = Lexicon::load();
    let mut rep = Reporter::new(&opts.lang);
    let toks = Lexer::new(&src, &lex, &mut rep).tokenize();
    let mut parser = Parser::new(&src, &lex, &mut rep, toks);
    let stmts = parser.parse_program();
    if rep.has_errors() {
        return Err(rep.render(&src));
    }
    Ok(fmt::format(&stmts, view, &lex))
}

/// Convenience for tests and examples.
pub fn run_file(path: &str, opts: &Options) -> Result<Outcome, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    run_source(&text, path, opts)
}
