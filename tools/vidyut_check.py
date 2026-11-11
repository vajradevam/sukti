#!/usr/bin/env python3
"""Network-optional Vidyut cross-check for Sūkti's transliteration tables.

What this does
--------------
1. OFFLINE (always, deterministic): a small set of representative transliteration
   mappings is checked against the transliteration tables in
   ``crates/sanskrit/src/translit.rs``. The "actual" side is produced either by
   the built ``sukti`` CLI (``translit`` subcommand) when the binary is present,
   or -- as a fallback -- by a faithful Python port of the same table logic that
   parses the ``const ...: Table`` blocks straight out of the Rust source.
   A mismatch here (or an engine that cannot be exercised at all) is a real
   failure and makes the script exit non-zero.

2. ONLINE (opt-in, informational only): when ``SUKTi_VIDYUT=1`` is set in the
   environment or ``--online`` is passed, query the crates.io API for the latest
   ``vidyut-lipi`` version and record it. Network errors are swallowed: this
   step NEVER fails CI and always degrades gracefully to "not available".

The script prints a Markdown evidence table on stdout. It exits non-zero only
for a real offline self-check mismatch; network availability never affects the
exit status.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
TRANSIT_RUST = REPO_ROOT / "crates" / "sanskrit" / "src" / "translit.rs"
CLI_CANDIDATES = [
    REPO_ROOT / "target" / "debug" / "sukti",
    REPO_ROOT / "target" / "release" / "sukti",
]
VIDYUT_URL = "https://crates.io/api/v1/crates/vidyut-lipi"

# (from, input, to, expected) -- expected values are the human-authored
# assertions documented in crates/sanskrit/src/translit.rs unit tests plus
# representative single-mapping spot-checks read from the tables themselves.
# They pin the current, reviewed conventions; a deliberate table change should
# force these vectors to be reviewed and updated.
VECTORS = [
    ("iast", "namaste", "devanagari", "नमस्ते"),
    ("iast", "rāmaḥ", "devanagari", "रामः"),
    ("iast", "jñāna", "devanagari", "ज्ञान"),
    ("iast", "aṣṭādhyāyī", "devanagari", "अष्टाध्यायी"),
    ("iast", "bhāṣā", "devanagari", "भाषा"),
    ("itrans", "namaste", "devanagari", "नमस्ते"),
    ("hk", "namaste", "devanagari", "नमस्ते"),
    ("slp1", "namaste", "devanagari", "नमस्ते"),
    ("devanagari", "नमस्ते", "iast", "namaste"),
    ("devanagari", "रामः", "iast", "rāmaḥ"),
    ("devanagari", "क्ष", "iast", "kṣa"),
    ("slp1", "f", "devanagari", "ऋ"),       # SLP1 vocalic ṛ
    ("hk", "G", "devanagari", "ङ्"),        # HK ṅa + virāma
    ("itrans", "~n", "devanagari", "ङ्"),   # ITRANS ṅa + virāma
]

# ---------------------------------------------------------------------------
# Fallback engine: parse the Rust tables and port the transliteration logic.
# ---------------------------------------------------------------------------

# From def() in translit.rs: anusvara / visarga / avagraha per scheme.
SPECIALS = {
    "iast": ("ṃ", "ḥ", "'"),
    "itrans": ("M", "H", "'"),
    "hk": ("M", "H", "'"),
    "slp1": ("M", "H", "'"),
}
PREFIX = {"iast": "IAST", "itrans": "ITRANS", "hk": "HK", "slp1": "SLP1"}


def parse_tables(path: Path):
    """Return {scheme: {cons, vowels, dep, special, anusvara, visarga, avagraha}}."""
    text = path.read_text(encoding="utf-8")
    tables = {}
    block_re = re.compile(r"const\s+(\w+)\s*:\s*Table\s*=\s*&\[(.*?)\];", re.S)
    pair_re = re.compile(r'\("((?:[^"\\]|\\.)*)",\s*"((?:[^"\\]|\\.)*)"\)')
    for name, body in block_re.findall(text):
        tables[name] = [(k, v) for k, v in pair_re.findall(body)]
    required = [f"{p}_{suffix}" for p in PREFIX.values()
                for suffix in ("CONS", "VOWELS", "DEP", "SPECIAL")]
    missing = [n for n in required if n not in tables]
    if missing:
        raise RuntimeError(f"could not parse table(s) in {path}: {', '.join(missing)}")
    defs = {}
    for scheme, prefix in PREFIX.items():
        a, v, av = SPECIALS[scheme]
        defs[scheme] = {
            "cons": tables[f"{prefix}_CONS"],
            "vowels": tables[f"{prefix}_VOWELS"],
            "dep": tables[f"{prefix}_DEP"],
            "special": tables[f"{prefix}_SPECIAL"],
            "anusvara": a,
            "visarga": v,
            "avagraha": av,
        }
    return defs


def _longest(rem: str, table):
    best = None  # (val, key, keylen)
    for key, val in table:
        if rem.startswith(key):
            n = len(key)
            if best is None or n > best[2]:
                best = (val, key, n)
    return best


def _tokenize(s: str, d):
    units = []  # (kind, payload)
    i = 0
    while i < len(s):
        rem = s[i:]
        m = _longest(rem, d["cons"])
        if m:
            units.append(("Cons", m[0]))
            i += m[2]
            continue
        m = _longest(rem, d["vowels"])
        if m:
            units.append(("Vowel", s[i : i + m[2]]))
            i += m[2]
            continue
        m = _longest(rem, d["special"])
        if m:
            units.append(("Special", m[0]))
            i += m[2]
            continue
        units.append(("Other", s[i]))
        i += 1
    return units


def _dev_from(s: str, d) -> str:
    units = _tokenize(s, d)
    out = []
    for j, (kind, val) in enumerate(units):
        if kind == "Cons":
            out.append(val)
            nxt = units[j + 1][0] if j + 1 < len(units) else None
            if nxt in (None, "Cons", "Other"):
                out.append("\u094d")  # virāma
        elif kind == "Vowel":
            prev_is_cons = j > 0 and units[j - 1][0] == "Cons"
            table = d["dep"] if prev_is_cons else d["vowels"]
            m = _longest(val, table)
            if m:
                out.append(m[0])
        elif kind == "Special":
            out.append(val)
        else:
            out.append("।" if val == ";" else val)
    return "".join(out)


def _dev_to(dev: str, d) -> str:
    cons = {}
    for key, val in d["cons"]:
        cons.setdefault(val, key)
    dep = {}
    for key, val in d["dep"]:
        if key:
            dep.setdefault(val, key)
    ind = {}
    for key, val in d["vowels"]:
        if key != "a":
            ind.setdefault(val, key)
    out = []
    chars = list(dev)
    i = 0
    while i < len(chars):
        c = chars[i]
        if c in cons:
            out.append(cons[c])
            nxt = chars[i + 1] if i + 1 < len(chars) else None
            if nxt == "\u094d":
                i += 1
            elif nxt in dep:
                pass
            else:
                out.append("a")
        elif c in dep:
            out.append(dep[c])
        elif c in ind:
            out.append(ind[c])
        elif c == "अ":
            out.append("a")
        elif c == "\u094d":
            pass
        elif c == "\u0902":
            out.append(d["anusvara"])
        elif c == "\u0903":
            out.append(d["visarga"])
        elif c == "\u093d":
            out.append(d["avagraha"])
        elif c in ("।", "॥"):
            out.append(";")
        elif "\u0966" <= c <= "\u096f":
            out.append(chr(ord("0") + (ord(c) - 0x0966)))
        else:
            out.append(c)
        i += 1
    return "".join(out)


def translit_source(s, frm, to, defs) -> str:
    if frm == to:
        return s
    dev = s if frm == "devanagari" else _dev_from(s, defs[frm])
    return dev if to == "devanagari" else _dev_to(dev, defs[to])


# ---------------------------------------------------------------------------
# Backends
# ---------------------------------------------------------------------------


def find_cli():
    for p in CLI_CANDIDATES:
        if p.is_file() and os.access(p, os.X_OK):
            return p
    return None


def translit_cli(binary, s, frm, to) -> str:
    import subprocess

    res = subprocess.run(
        [str(binary), "translit", s, "--from", frm, "--to", to],
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=15,
    )
    if res.returncode != 0:
        raise RuntimeError(f"CLI exited {res.returncode}: {res.stderr.strip()}")
    return res.stdout.rstrip("\n")


def query_online():
    """Return (status, version, detail). Never raises; never gates CI."""
    try:
        req = urllib.request.Request(
            VIDYUT_URL,
            headers={"User-Agent": "sukti-vidyut-check/1.0 (offline-safe CI cross-check)"},
        )
        with urllib.request.urlopen(req, timeout=5) as resp:
            payload = json.load(resp)
        crate = payload.get("crate", {}) if isinstance(payload, dict) else {}
        version = crate.get("max_version") or crate.get("newest_version")
        if not version:
            return ("unavailable", "—", "response had no version field")
        return ("ok", str(version), VIDYUT_URL)
    except Exception as exc:  # noqa: BLE001 - network errors must be non-fatal
        return ("unavailable", "—", f"{type(exc).__name__}: {exc}")


# ---------------------------------------------------------------------------
# Markdown rendering
# ---------------------------------------------------------------------------


def md_cell(value: str) -> str:
    return str(value).replace("|", "\\|").replace("\n", " ")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--online",
        action="store_true",
        help="also query crates.io for the latest vidyut-lipi version (informational)",
    )
    args = ap.parse_args()

    online_requested = args.online or os.environ.get("SUKTi_VIDYUT") == "1" \
        or os.environ.get("SUKTI_VIDYUT") == "1"

    # Pick an engine for the offline self-check.
    cli = find_cli()
    defs = None
    if cli is not None:
        engine = f"built CLI (`{cli.relative_to(REPO_ROOT)}`)"
    else:
        try:
            defs = parse_tables(TRANSIT_RUST)
        except Exception as exc:  # noqa: BLE001
            print(f"ERROR: no CLI binary and cannot parse transliteration tables: {exc}",
                  file=sys.stderr)
            return 2
        engine = f"source-table parser (`{TRANSIT_RUST.relative_to(REPO_ROOT)}`)"

    rows = []
    failures = []
    for i, (frm, text, to, expected) in enumerate(VECTORS, 1):
        try:
            actual = (translit_cli(cli, text, frm, to) if cli is not None
                      else translit_source(text, frm, to, defs))
            ok = actual == expected
        except Exception as exc:  # noqa: BLE001
            actual = f"ERROR: {exc}"
            ok = False
        rows.append((i, frm, text, to, expected, actual, ok))
        if not ok:
            failures.append(i)

    total = len(rows)
    passed = total - len(failures)

    print("# Vidyut cross-check — Sūkti transliteration tables")
    print()
    print("This is a **bounded, offline self-check** of the small transliteration")
    print("tables baked into `crates/sanskrit/src/translit.rs`. It is not full Vidyut")
    print("parity. Scope and limitations are described in README.md.")
    print()
    print(f"Offline engine: {engine}")
    print()
    print("## Offline self-check (deterministic, always runs)")
    print()
    print("| # | From | Input | To | Expected | Actual | Result |")
    print("|---:|---|---|---|---|---|---|")
    for i, frm, text, to, expected, actual, ok in rows:
        print(f"| {i} | {md_cell(frm)} | {md_cell(text)} | {md_cell(to)} | "
              f"{md_cell(expected)} | {md_cell(actual)} | {'PASS' if ok else 'FAIL'} |")
    print()
    print(f"Offline result: **{passed}/{total} PASS**"
          + (f" — FAILING rows: {', '.join(map(str, failures))}" if failures else ""))
    print()

    print("## Online Vidyut metadata (optional, informational — never gates CI)")
    print()
    if online_requested:
        status, version, detail = query_online()
        print("| Item | Value |")
        print("|---|---|")
        print(f"| Requested | yes (`SUKTi_VIDYUT=1` / `--online`) |")
        print(f"| Endpoint | `{VIDYUT_URL}` |")
        print(f"| Status | {md_cell(status)} |")
        print(f"| Latest `vidyut-lipi` | {md_cell(version)} |")
        print(f"| Detail | {md_cell(detail)} |")
        print()
        if status == "ok":
            print(f"Recorded latest published `vidyut-lipi`: **{version}**.")
        else:
            print("Network query unavailable; degraded gracefully (no CI failure).")
    else:
        print("| Item | Value |")
        print("|---|---|")
        print("| Requested | no (offline mode) |")
        print("| Status | not queried |")
        print()
        print("Set `SUKTi_VIDYUT=1` or pass `--online` to record the latest")
        print("`vidyut-lipi` version from crates.io. This never gates CI.")
    print()

    if failures:
        print(f"VERDICT: FAIL ({len(failures)} offline mismatch(es)).", file=sys.stderr)
        return 1
    print("VERDICT: PASS (offline self-check).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
