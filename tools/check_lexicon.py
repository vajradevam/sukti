#!/usr/bin/env python3
"""Dev-time validation of lexicon/lexicon.toml (no runtime dependency).

Checks: structure, required fields, unique canonical ITRANS spellings, valid
status values, trilingual messages, and NFC normalization. Does not claim
grammatical approval.
"""
import sys, unicodedata
try:
    import tomllib
except ImportError:
    print("python 3.11+ required for tomllib"); sys.exit(2)

p = "lexicon/lexicon.toml"
with open(p, "rb") as f:
    d = tomllib.load(f)

errs = []
seen = {}
for i, e in enumerate(d.get("entry", [])):
    for k in ("id", "canonical", "iast", "kind", "gloss_en", "gloss_sa", "status"):
        if not e.get(k):
            errs.append(f"entry[{i}] missing {k}")
    if e.get("status") not in {"validated-by-tool", "pending-scholar-review", "scholar-approved"}:
        errs.append(f"entry[{i}] bad status {e.get('status')}")
    if e.get("canonical") != unicodedata.normalize("NFC", e.get("canonical", "")):
        errs.append(f"entry[{i}] canonical not NFC")
    for k in ("canonical", "iast"):
        v = e.get(k, "")
        key = (k, v)  # uniqueness is per field: canonical often equals iast
        if key in seen:
            errs.append(f"duplicate {k} '{v}' in entry[{i}] and entry[{seen[key]}]")
        seen[key] = i

for m in d.get("message", []):
    for k in ("code", "en", "sa", "hi"):
        if not m.get(k):
            errs.append(f"message {m.get('code')} missing {k}")

print(f"checked {len(d.get('entry', []))} lexicon entries and {len(d.get('message', []))} messages")
if errs:
    for e in errs:
        print("ERROR:", e)
    sys.exit(1)
print("OK")
