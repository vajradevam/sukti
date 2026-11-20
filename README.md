# Sūkti

Sūkti is a little programming language. The source is plain ASCII (ITRANS romanization), and the grammar borrows a couple of ideas from Sanskrit that the compiler actually uses.

```sukti
vada("namaste, vizva!");
```

```
./target/debug/sukti run hello.sukti
namaste, vizva!
```

## Why I wrote it

I wanted parameters that say what they mean. In Sūkti you can mark a parameter as `kartA` (the doer) or `karma` (the thing being acted on), then pass arguments in whatever order reads best. The resolver sorts them out before the program runs.

```sukti
kriyA preshaya(kartA sender, karma letter) {
    phalam sender + " -> " + letter;
}
vada(preshaya(karma: "patram", kartA: "rAma"));
```

Sūtra clauses give you a general rule and exceptions with a fixed precedence. There is also a second kāraka layer that recognises inflected forms, although it stays deliberately small because Sanskrit forms are often ambiguous. When that happens you get an error instead of a guess.

It is otherwise a normal language. You get bigints, floats, strings, lists, maps, classes, closures, exceptions, modules, and tasks with channels.

## Build and run

```
./scripts/bootstrap.sh
cargo build -p sukti-bin
./target/debug/sukti run examples/01-namaste.sukti
./target/debug/sukti test examples
./target/debug/sukti repl
```

The binary lands at `target/debug/sukti`. The bootstrap script also runs the test suite.

## A quick tour

`astu` makes a mutable variable and `nitya` makes a constant. Statements end in `;` and blocks use braces.

```sukti
astu counter = 0;
nitya greeting = "namaste";
counter = counter + 1;
```

`yadi` and `anyathA` branch, `yAvat` loops, and `pratyekam x antaH 1..4` walks a range. `anuvarta` is continue and `virama` is break. `phalam` returns a value. Functions use `kriyA`, classes use `varga` with `svayam` for the current object, and `yatna` / `doShe` / `ante` handle exceptions.

There are a few modules in the standard library. `gaNita` has math, `su` has file and JSON helpers plus regex and tasks, and `parIkShA` has assertions. `saMskRRita` holds the Sanskrit text tools: transliteration, sandhi candidates, declensions, pratyāhāras and metre. Source code is always ITRANS. If you ask `saMskRRita.rUpAntaraNa(...)` for Devanagari or IAST you get a string back, which is useful for text work but does not turn it into an alternate source script.

## Commands

| Command | Use |
|---|---|
| `sukti run file.sukti` | run it |
| `sukti check file.sukti` | parse and check without running |
| `sukti fmt file.sukti` | format, add `--inplace` to overwrite |
| `sukti test examples` | compare examples with their `.expected` files |
| `sukti doc file.sukti` | pull out `///` comments |
| `sukti repl` | interactive prompt, `exit` quits |
| `sukti lsp` and `sukti dap` | language server and debug adapter over stdio |
| `sukti translit --from X --to Y "text"` | convert between scripts |

Useful flags are `--jit=on|off|aggressive`, `--jit-threshold N`, `--dump-jit`, `--prakriya`, `--lang en|sa|hi` and `--max-steps N`.

## Editor and playground

The VS Code extension is in `editors/vscode`. It has highlighting, snippets and an LSP client. Build it with `npm install --ignore-scripts` then `npm run compile`, open the folder and press F5. It starts `sukti lsp` and you can point the `sukti.path` setting at your binary if it is not on PATH.

The playground in `playground/` runs a WASM build of the interpreter:

```
cd playground
python3 -m http.server 8080
```

Open `http://localhost:8080/`. Click a line number to set a breakpoint. Step and Continue walk a recorded trace, so stepping shows values at each statement but it is not pausing live code. The WASM file is checked in so this works without a network connection, and `bash scripts/build-wasm.sh` rebuilds it.

## How it fits together

```
source -> lexer -> parser -> AST -> resolver -> VM
                                      |
                                   Tier 1 JIT
```

The workspace has six crates. `sukti-sanskrit` holds the text tables. `sukti` has the front end, VM, standard library and CLI. `sukti-jit` compiles hot integer functions with Cranelift and deopts to the VM when an operation is too large. `sukti-lsp` serves LSP and DAP. `sukti-wasm` is the browser build. `sukti-bin` links everything into one executable.

This is a tiered engine, not a toy. On my machine the JIT is around 664 times faster on recursive fibonacci, 57 times on a plain loop and 53 times on a nested loop. Run `bash bench/run_bench.sh` to see your own numbers. Code that uses lists or method calls is left in the interpreter and pays no profiling cost.

The on-disk cache stores compilation plans and deopt metadata, not machine code. The kāraka Layer B subset is small on purpose.

## Tests and vocabulary

```
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
python3 tools/check_lexicon.py
python3 tools/vidyut_check.py
```

There are 14 golden examples, one of them a 503-line glossary app and another a tokenizer written in Sūkti. The conformance suite has 393 cases, the diagnostic snapshots cover 67 bad snippets in three languages, and the five frozen spec blocks live in `tests/spec_blocks.txt`.

Keywords, roles and diagnostic messages all live in `lexicon/lexicon.toml`. After editing it, run `python3 tools/gen_review.py` and `python3 tools/gen_grammar.py` and the review table, grammar and editor snippets update themselves.

The review table in `docs/review/` is a request for feedback, not a stamp of approval. Nothing there is marked scholar-approved. The examples avoid mantras, deity names and sacred verses, and the design does not claim Sanskrit is better for computing. Pāṇinian ideas are used as semantics, with the simplifications written down.

## License

MIT. See `LICENSE`.
