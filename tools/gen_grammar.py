#!/usr/bin/env python3
"""Generate the VS Code TextMate grammar + snippets from lexicon/lexicon.toml.

The language is ITRANS-only: keywords, roles and types come from the lexicon's
`canonical` (ASCII ITRANS) field, and snippets use `;` terminators.
"""
import json, os, tomllib, re

with open('lexicon/lexicon.toml','rb') as f: d = tomllib.load(f)

def esc(s): return re.escape(s)
def alt(xs): return '(?:' + '|'.join(esc(x) for x in xs) + ')'

keywords = [e['canonical'] for e in d['entry'] if e['kind'] == 'keyword']
keywords += [a for e in d['entry'] for a in e['aliases']]
roles = [e['canonical'] for e in d['entry'] if e['kind'] == 'role']
types = [e['canonical'] for e in d['entry'] if e['kind'] == 'type']

grammar = {
  '$schema': 'https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json',
  'name': 'Sūkti',
  'scopeName': 'source.sukti',
  'patterns': [
    {'include': '#comments'},
    {'name': 'keyword.control.sukti', 'match': '\\b' + alt(keywords) + '\\b'},
    {'name': 'entity.name.tag.karaka.sukti', 'match': '\\b' + alt(roles) + '\\b'},
    {'name': 'support.type.sukti', 'match': '\\b' + alt(types) + '\\b'},
    {'include': '#strings'},
    {'name': 'constant.numeric.sukti', 'match': '\\b(?:[0-9]+(?:\\.[0-9]+)?)'},
    {'name': 'punctuation.terminator.sukti', 'match': ';'},
  ],
  'repository': {
    'comments': {'patterns': [
      {'name': 'comment.line.sukti', 'match': '//.*$'},
      {'name': 'comment.block.sukti', 'begin': '/\\*', 'end': '\\*/'},
      {'name': 'comment.line.sanskrit.sukti', 'match': '//\\s*(?:TippaNI|Note):.*'},
    ]},
    'strings': {'name': 'string.quoted.sukti', 'begin': '"', 'end': '"', 'patterns': [
      {'name': 'constant.character.escape.sukti', 'match': '\\\\.'},
      {'name': 'meta.interpolation.sukti', 'begin': '\\{', 'end': '\\}'},
    ]},
  },
}
os.makedirs('editors/vscode/syntaxes', exist_ok=True)
with open('editors/vscode/syntaxes/sukti.tmLanguage.json','w',encoding='utf-8') as f:
    json.dump(grammar, f, ensure_ascii=False, indent=2)

snips = {
  'Hello': {'prefix': 'vada', 'body': ['vada("${1:namaste, vishva!}");'], 'description': 'print'},
  'Function': {'prefix': 'kriyA', 'body': ['kriyA ${1:nAma}(${2:pairAma}) {', '\tphalam ${3:0};', '}'], 'description': 'function'},
  'Kāraka function': {'prefix': 'kArakakriyA', 'body': ['kriyA ${1:nAma}(kartA ${2:ka}, karma ${3:kha}) {', '\tphalam ${2:ka};', '}'], 'description': 'kāraka function'},
  'Sūtra': {'prefix': 'sUtra', 'body': ['sUtra ${1:nAma}(${2:va}) {', '\tphalam ${3:0};', '}'], 'description': 'sūtra rule'},
  'Try': {'prefix': 'yatna', 'body': ['yatna {', '\t${1:vada("…");}', '} doShe (I) {', '\tvada(I.sandesha);', '} ante {', '\t${2:vada("cleanup");}', '}'], 'description': 'exceptions'},
}
os.makedirs('editors/vscode/snippets', exist_ok=True)
with open('editors/vscode/snippets/sukti.json','w',encoding='utf-8') as f:
    json.dump(snips, f, ensure_ascii=False, indent=2)

# Patch only the generated contributions into the existing manifest. The
# manifest also carries the LSP client wiring (main, scripts, dependencies,
# configuration). Rewriting it wholesale used to silently drop that wiring.
manifest_path = 'editors/vscode/package.json'
with open(manifest_path, encoding='utf-8') as f:
    manifest = json.load(f)
contributes = manifest.setdefault('contributes', {})
contributes['languages'] = [
    {'id': 'sukti', 'aliases': ['Sūkti', 'sUkti'], 'extensions': ['.sukti', '.sk']}
]
contributes['grammars'] = [
    {'language': 'sukti', 'scopeName': 'source.sukti', 'path': './syntaxes/sukti.tmLanguage.json'}
]
contributes['snippets'] = [
    {'language': 'sukti', 'path': './snippets/sukti.json'}
]
with open(manifest_path, 'w', encoding='utf-8') as f:
    json.dump(manifest, f, ensure_ascii=False, indent=2)
    f.write('\n')
print('generated VS Code grammar + snippets from lexicon; preserved LSP client manifest')
