//! Core standard library and the flagship `saMskRRita` module.

use crate::value::*;
use crate::vm::VM;
use num_bigint::BigInt;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{self, BufRead};
use std::rc::Rc;

struct Simple(String, fn(&mut VM, Vec<Value>) -> VResult<Value>);
fn native(n: &str, f: NativeFn) -> Rc<dyn Native> {
    Rc::new(Simple(n.to_string(), f))
}
impl Native for Simple {
    fn name(&self) -> &str {
        &self.0
    }
    fn call(&self, vm: &mut VM, args: Vec<Value>) -> VResult<Value> {
        (self.1)(vm, args)
    }
}

struct Method {
    recv: Value,
    name: String,
    f: fn(&Value, &mut VM, Vec<Value>) -> VResult<Value>,
}
impl Native for Method {
    fn name(&self) -> &str {
        &self.name
    }
    fn call(&self, vm: &mut VM, args: Vec<Value>) -> VResult<Value> {
        (self.f)(&self.recv, vm, args)
    }
}

fn method(recv: &Value, name: &str, f: fn(&Value, &mut VM, Vec<Value>) -> VResult<Value>) -> Value {
    Value::Native(Rc::new(Method {
        recv: recv.clone(),
        name: name.to_string(),
        f,
    }))
}

/// Register built-ins and modules into a fresh VM.
pub fn register(vm: &mut VM) {
    vm.add_native("vada", native("vada", print));
    vm.add_native("paTha", native("paTha", read_line));
    vm.add_native("doSha", native("doSha", make_fault));
    vm.add_native("asti", native("asti", type_of));
    vm.add_native("rUpAntara", native("rUpAntara", to_string_native));

    let math = module(
        "gaNita",
        vec![
            ("vargamUla", sqrt),
            ("shakti", pow),
            ("nirapekSha", abs),
            ("nyUnatama", min),
            ("adhikatama", max),
            ("gola", round),
            ("pi", pi),
            ("e", euler),
        ],
    );
    vm.modules.insert("gaNita".into(), math.clone());
    vm.globals
        .borrow_mut()
        .define("gaNita", Value::Module(math), false);

    let sans = module(
        "saMskRRita",
        vec![
            ("rUpAntaraNa", transliterate),
            ("sandhi", sandhi_join),
            ("sandhivichchheda", sandhi_split),
            ("pratyAhAra", pratyahara),
            ("akSharANi", aksharas),
            ("mAtrA", weights),
            ("chhandaH", metre),
            ("vibhakti", decline),
            ("vibhaktiparichchheda", parse_form),
        ],
    );
    vm.modules.insert("saMskRRita".into(), sans.clone());
    vm.globals
        .borrow_mut()
        .define("saMskRRita", Value::Module(sans), false);

    let util = module(
        "su",
        vec![
            ("virAma", yield_native),
            ("chainala", channel),
            ("joDa", join_tasks),
            ("yAdRRichchhika", random),
            ("bIja", seed),
            ("samaya", now),
            ("paThana", fs_read),
            ("lekhana", fs_write),
            ("astitva", fs_exists),
            ("vishleShaNa", json_parse),
            ("nirmANa", json_stringify),
            ("milAna", regex_match),
        ],
    );
    vm.modules.insert("su".into(), util.clone());
    vm.globals
        .borrow_mut()
        .define("su", Value::Module(util), false);

    let test = module(
        "parIkShA",
        vec![
            ("sama", assert_eq_native),
            ("satya", assert_true),
            ("viphala", fail),
        ],
    );
    vm.modules.insert("parIkShA".into(), test.clone());
    vm.globals
        .borrow_mut()
        .define("parIkShA", Value::Module(test), false);
}

type NativeFn = fn(&mut VM, Vec<Value>) -> VResult<Value>;

fn module(name: &str, entries: Vec<(&str, NativeFn)>) -> Rc<Module> {
    let env = Env::root();
    for (n, f) in entries {
        env.borrow_mut()
            .define(n, Value::Native(Rc::new(Simple(n.to_string(), f))), false);
    }
    Rc::new(Module {
        name: name.to_string(),
        env,
    })
}

// -------------------------------------------------------------- core I/O

fn print(vm: &mut VM, args: Vec<Value>) -> VResult<Value> {
    let s = args
        .iter()
        .map(|v| v.display())
        .collect::<Vec<_>>()
        .join(" ");
    vm.output.push_str(&s);
    vm.output.push('\n');
    Ok(Value::Null)
}
fn read_line(_vm: &mut VM, _args: Vec<Value>) -> VResult<Value> {
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| Fault::new("E0002", e.to_string()))?;
    Ok(Value::Str(line.trim_end_matches(['\n', '\r']).to_string()))
}
fn make_fault(vm: &mut VM, args: Vec<Value>) -> VResult<Value> {
    let msg = args.first().map(|v| v.display()).unwrap_or_default();
    Ok(vm.fault_value(&Fault::new("doSha", msg)))
}
fn type_of(_vm: &mut VM, args: Vec<Value>) -> VResult<Value> {
    Ok(Value::Str(
        args.first()
            .map(|v| v.type_name().to_string())
            .unwrap_or_default(),
    ))
}
fn to_string_native(_vm: &mut VM, args: Vec<Value>) -> VResult<Value> {
    Ok(Value::Str(
        args.first().map(|v| v.display()).unwrap_or_default(),
    ))
}

// -------------------------------------------------------------- math

fn farg(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Int(i)) => i.to_string().parse().ok(),
        Some(Value::Float(f)) => Some(*f),
        _ => None,
    }
}
fn sqrt(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    Ok(Value::Float(farg(a.first()).unwrap_or(0.0).sqrt()))
}
fn pow(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    Ok(Value::Float(
        farg(a.first())
            .unwrap_or(0.0)
            .powf(farg(a.get(1)).unwrap_or(0.0)),
    ))
}
fn abs(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    Ok(match a.first() {
        Some(Value::Int(i)) => Value::Int(if i.to_string().starts_with('-') {
            -i
        } else {
            i.clone()
        }),
        v => Value::Float(farg(v).unwrap_or(0.0).abs()),
    })
}
fn min(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    Ok(a.into_iter()
        .min_by(|x, y| {
            farg(Some(x))
                .unwrap_or(0.0)
                .partial_cmp(&farg(Some(y)).unwrap_or(0.0))
                .unwrap()
        })
        .unwrap_or(Value::Null))
}
fn max(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    Ok(a.into_iter()
        .max_by(|x, y| {
            farg(Some(x))
                .unwrap_or(0.0)
                .partial_cmp(&farg(Some(y)).unwrap_or(0.0))
                .unwrap()
        })
        .unwrap_or(Value::Null))
}
fn round(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    Ok(Value::Int(BigInt::from(
        farg(a.first()).unwrap_or(0.0).round() as i64,
    )))
}
fn pi(_: &mut VM, _: Vec<Value>) -> VResult<Value> {
    Ok(Value::Float(std::f64::consts::PI))
}
fn euler(_: &mut VM, _: Vec<Value>) -> VResult<Value> {
    Ok(Value::Float(std::f64::consts::E))
}

// -------------------------------------------------------------- Sanskrit module

fn arg_str(a: &[Value], i: usize) -> String {
    a.get(i).map(|v| v.display()).unwrap_or_default()
}

/// Convert an ITRANS program string to Devanagari before handing it to the
/// scholarly tables (which are stored in that script internally).
fn dev(s: impl AsRef<str>) -> String {
    sukti_sanskrit::transliterate(
        s.as_ref(),
        sukti_sanskrit::Script::Itrans,
        sukti_sanskrit::Script::Devanagari,
    )
}

/// The scholarly tables are stored in Devanagari; convert their results at the
/// language boundary so every value a Sūkti program sees is ITRANS.
fn itrans(s: impl AsRef<str>) -> String {
    sukti_sanskrit::transliterate(
        s.as_ref(),
        sukti_sanskrit::Script::Devanagari,
        sukti_sanskrit::Script::Itrans,
    )
}

fn transliterate(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    use sukti_sanskrit::{translit::Script, transliterate};
    let from = Script::parse(&arg_str(&a, 1)).unwrap_or(Script::Devanagari);
    let to = Script::parse(&arg_str(&a, 2)).unwrap_or(Script::Iast);
    Ok(Value::Str(transliterate(&arg_str(&a, 0), from, to)))
}
fn sandhi_join(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let c = sukti_sanskrit::sandhi::join(&dev(arg_str(&a, 0)), &dev(arg_str(&a, 1)));
    Ok(Value::List(Rc::new(RefCell::new(
        c.into_iter().map(|x| Value::Str(itrans(x))).collect(),
    ))))
}
fn sandhi_split(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let c = sukti_sanskrit::sandhi::split(&dev(arg_str(&a, 0)));
    Ok(Value::List(Rc::new(RefCell::new(
        c.into_iter()
            .map(|(x, y)| Value::Tuple(Rc::new(vec![Value::Str(itrans(x)), Value::Str(itrans(y))])))
            .collect(),
    ))))
}
fn pratyahara(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let s = sukti_sanskrit::pratyahara::expand_string(&dev(arg_str(&a, 0))).unwrap_or_default();
    Ok(Value::Str(itrans(s)))
}
fn aksharas(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let v = sukti_sanskrit::metre::akshara_split(&dev(arg_str(&a, 0)));
    Ok(Value::List(Rc::new(RefCell::new(
        v.into_iter().map(|x| Value::Str(itrans(x))).collect(),
    ))))
}
fn weights(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    Ok(Value::Str(itrans(sukti_sanskrit::metre::laghu_guru(&dev(
        arg_str(&a, 0),
    )))))
}
fn metre(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    match sukti_sanskrit::metre::detect_metre(&dev(arg_str(&a, 0))) {
        Some(m) => Ok(Value::Str(itrans(m))),
        None => Ok(Value::Null),
    }
}
fn decline(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    use sukti_sanskrit::declension::*;
    let class = if arg_str(&a, 1) == "aa" || arg_str(&a, 1) == "ā" {
        StemClass::Aa
    } else {
        StemClass::A
    };
    let gender = match arg_str(&a, 2).as_str() {
        "m" | "masc" | "pu" => Gender::Masc,
        "n" | "neut" | "napu" => Gender::Neut,
        _ => Gender::Fem,
    };
    let case = match arg_str(&a, 4).as_str() {
        "acc" | "2" => Case::Acc,
        "ins" | "3" => Case::Ins,
        "dat" | "4" => Case::Dat,
        "abl" | "5" => Case::Abl,
        "gen" | "6" => Case::Gen,
        "loc" | "7" => Case::Loc,
        "voc" | "8" => Case::Voc,
        _ => Case::Nom,
    };
    Ok(Value::Str(itrans(sukti_sanskrit::declension::decline(
        &dev(arg_str(&a, 0)),
        class,
        gender,
        Number::Sg,
        case,
    ))))
}
fn parse_form(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let forms = sukti_sanskrit::declension::parse_inflected(
        &dev(arg_str(&a, 0)),
        sukti_sanskrit::declension::Number::Sg,
    )
    .unwrap_or_default();
    Ok(Value::List(Rc::new(RefCell::new(
        forms
            .into_iter()
            .map(|f| Value::Str(itrans(format!("{}/{}", f.stem, f.case as u8))))
            .collect(),
    ))))
}

// -------------------------------------------------------------- utilities

fn yield_native(_: &mut VM, _: Vec<Value>) -> VResult<Value> {
    Ok(Value::Null)
}
fn channel(vm: &mut VM, _: Vec<Value>) -> VResult<Value> {
    let id = vm.channels.len();
    vm.channels.push(Rc::new(RefCell::new(Channel {
        messages: VecDeque::new(),
        closed: false,
        blocked_task: None,
    })));
    Ok(Value::Channel(vm.channels[id].clone(), id))
}
fn join_tasks(_: &mut VM, _: Vec<Value>) -> VResult<Value> {
    Ok(Value::Null)
}
fn random(vm: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let n = farg(a.first()).unwrap_or(1.0) as u64;
    vm.rng_state = vm
        .rng_state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    Ok(Value::Int(BigInt::from(if n == 0 {
        0
    } else {
        (vm.rng_state >> 33) % n
    })))
}
fn seed(vm: &mut VM, a: Vec<Value>) -> VResult<Value> {
    vm.rng_state = farg(a.first()).unwrap_or(0.0) as u64;
    Ok(Value::Null)
}
fn now(_: &mut VM, _: Vec<Value>) -> VResult<Value> {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(Value::Int(BigInt::from(d.as_millis() as i64)))
}
fn fs_read(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    std::fs::read_to_string(arg_str(&a, 0))
        .map(Value::Str)
        .map_err(|e| Fault::new("E0002", e.to_string()))
}
fn fs_write(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    std::fs::write(arg_str(&a, 0), arg_str(&a, 1))
        .map(|_| Value::Null)
        .map_err(|e| Fault::new("E0002", e.to_string()))
}
fn fs_exists(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    Ok(Value::Bool(std::path::Path::new(&arg_str(&a, 0)).exists()))
}
fn json_parse(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let v: serde_json::Value =
        serde_json::from_str(&arg_str(&a, 0)).map_err(|e| Fault::new("E0002", e.to_string()))?;
    from_json(&v)
}
fn json_stringify(_vm: &mut VM, a: Vec<Value>) -> VResult<Value> {
    Ok(Value::Str(
        to_json(a.first().unwrap_or(&Value::Null)).to_string(),
    ))
}
fn regex_match(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let re = regex::Regex::new(&arg_str(&a, 0)).map_err(|e| Fault::new("E0002", e.to_string()))?;
    Ok(Value::Bool(re.is_match(&arg_str(&a, 1))))
}
fn assert_eq_native(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let ok = a.len() >= 2 && value_eq(&a[0], &a[1]);
    if ok {
        Ok(Value::Bool(true))
    } else {
        Err(Fault::new("E0017", "assertion failed: values differ"))
    }
}
fn assert_true(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    if a.first().map_or(false, |v| v.truthy()) {
        Ok(Value::Bool(true))
    } else {
        Err(Fault::new("E0017", "assertion failed"))
    }
}
fn fail(_: &mut VM, a: Vec<Value>) -> VResult<Value> {
    Err(Fault::new("E0017", arg_str(&a, 0)))
}

fn from_json(v: &serde_json::Value) -> VResult<Value> {
    Ok(match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(BigInt::from(i))
            } else {
                Value::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => Value::Str(s.clone()),
        serde_json::Value::Array(a) => Value::List(Rc::new(RefCell::new(
            a.iter().map(from_json).collect::<VResult<Vec<_>>>()?,
        ))),
        serde_json::Value::Object(o) => Value::Map(Rc::new(RefCell::new(
            o.iter()
                .map(|(k, v)| Ok((Value::Str(k.clone()), from_json(v)?)))
                .collect::<VResult<Vec<_>>>()?,
        ))),
    })
}
fn to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::Number(
            i.to_string()
                .parse::<i64>()
                .map(Into::into)
                .unwrap_or_else(|_| serde_json::Number::from(0)),
        ),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::Str(s) => serde_json::Value::String(s.clone()),
        Value::List(l) => serde_json::Value::Array(l.borrow().iter().map(|x| to_json(x)).collect()),
        Value::Tuple(t) => serde_json::Value::Array(t.iter().map(|x| to_json(x)).collect()),
        Value::Map(m) => serde_json::Value::Object(
            m.borrow()
                .iter()
                .map(|(k, v)| (k.display(), to_json(v)))
                .collect(),
        ),
        _ => serde_json::Value::String(v.display()),
    }
}

// -------------------------------------------------------------- methods

pub fn get_method(recv: Value, name: &str) -> Option<Value> {
    let f: fn(&Value, &mut VM, Vec<Value>) -> VResult<Value> = match (&recv, name) {
        (Value::Str(_), "dairghya" | "len") => str_len,
        (Value::Str(_), "vibhAjana" | "split") => str_split,
        (Value::Str(_), "pratisthApana" | "replace") => str_replace,
        (Value::Str(_), "adhaH" | "lower") => str_lower,
        (Value::Str(_), "upari" | "upper") => str_upper,
        (Value::Str(_), "chhaTanA" | "trim") => str_trim,
        (Value::Str(_), "khaNDa" | "slice") => list_slice,
        (Value::List(_), "dairghya" | "len") => list_len,
        (Value::List(_), "prakShepa" | "push") => list_push,
        (Value::List(_), "prApta" | "get") => list_get,
        (Value::List(_), "viloma" | "reverse") => list_reverse,
        (Value::List(_), "vinyAsa" | "sort") => list_sort,
        (Value::List(_), "khaNDa" | "slice") => list_slice,
        (Value::List(_), "yojanA" | "join") => list_join,
        (Value::Map(_), "dairghya" | "len") => map_len,
        (Value::Map(_), "prApta" | "get") => map_get,
        (Value::Map(_), "prakShepa" | "set") => map_set,
        (Value::Map(_), "kIsUchI" | "keys") => map_keys,
        (Value::Channel(..), "preShaya" | "send") => chan_send,
        (Value::Channel(..), "prApta" | "recv") => chan_recv,
        _ => return None,
    };
    Some(method(&recv, name, f))
}

fn idx(a: &[Value], i: usize) -> usize {
    a.get(i)
        .and_then(|v| match v {
            Value::Int(n) => n.to_string().parse().ok(),
            _ => None,
        })
        .unwrap_or(0)
}
fn str_len(r: &Value, _: &mut VM, _: Vec<Value>) -> VResult<Value> {
    Ok(Value::Int(BigInt::from(match r {
        Value::Str(s) => s.chars().count(),
        _ => 0,
    })))
}
fn str_split(r: &Value, _: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let s = match r {
        Value::Str(s) => s.as_str(),
        _ => "",
    };
    let sep = arg_str(&a, 0);
    let parts: Vec<Value> = if sep.is_empty() {
        s.chars().map(|c| Value::Str(c.to_string())).collect()
    } else {
        s.split(&sep).map(|x| Value::Str(x.to_string())).collect()
    };
    Ok(Value::List(Rc::new(RefCell::new(parts))))
}
fn str_replace(r: &Value, _: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let s = match r {
        Value::Str(s) => s.as_str(),
        _ => "",
    };
    Ok(Value::Str(s.replace(&arg_str(&a, 0), &arg_str(&a, 1))))
}
fn str_lower(r: &Value, _: &mut VM, _: Vec<Value>) -> VResult<Value> {
    Ok(Value::Str(match r {
        Value::Str(s) => s.to_lowercase(),
        _ => String::new(),
    }))
}
fn str_upper(r: &Value, _: &mut VM, _: Vec<Value>) -> VResult<Value> {
    Ok(Value::Str(match r {
        Value::Str(s) => s.to_uppercase(),
        _ => String::new(),
    }))
}
fn str_trim(r: &Value, _: &mut VM, _: Vec<Value>) -> VResult<Value> {
    Ok(Value::Str(match r {
        Value::Str(s) => s.trim().to_string(),
        _ => String::new(),
    }))
}
fn list_len(r: &Value, _: &mut VM, _: Vec<Value>) -> VResult<Value> {
    Ok(Value::Int(BigInt::from(match r {
        Value::List(l) => l.borrow().len(),
        _ => 0,
    })))
}
fn list_push(r: &Value, _: &mut VM, a: Vec<Value>) -> VResult<Value> {
    if let Value::List(l) = r {
        l.borrow_mut().extend(a);
    }
    Ok(Value::Null)
}
fn list_get(r: &Value, _vm: &mut VM, a: Vec<Value>) -> VResult<Value> {
    if let Value::List(l) = r {
        let i = idx(&a, 0);
        let l = l.borrow();
        return l
            .get(i)
            .cloned()
            .ok_or_else(|| Fault::new("E0013", format!("index {i}")));
    }
    Err(Fault::new(
        "E0008",
        format!("expected list, found {}", r.type_name()),
    ))
}
fn list_reverse(r: &Value, _: &mut VM, _: Vec<Value>) -> VResult<Value> {
    if let Value::List(l) = r {
        l.borrow_mut().reverse();
    }
    Ok(r.clone())
}
#[allow(clippy::unnecessary_sort_by)]
fn list_sort(r: &Value, _: &mut VM, _: Vec<Value>) -> VResult<Value> {
    if let Value::List(l) = r {
        l.borrow_mut().sort_by(|a, b| a.display().cmp(&b.display()));
    }
    Ok(r.clone())
}
fn list_slice(r: &Value, _: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let (from, to) = (idx(&a, 0), idx(&a, 1));
    let v: Vec<Value> = match r {
        Value::List(l) => {
            let l = l.borrow();
            l[from.min(l.len())..to.max(from).min(l.len())].to_vec()
        }
        Value::Str(s) => {
            let c: Vec<char> = s.chars().collect();
            c[from.min(c.len())..to.max(from).min(c.len())]
                .iter()
                .map(|c| Value::Str(c.to_string()))
                .collect()
        }
        _ => Vec::new(),
    };
    Ok(Value::List(Rc::new(RefCell::new(v))))
}
fn list_join(r: &Value, _: &mut VM, a: Vec<Value>) -> VResult<Value> {
    let sep = arg_str(&a, 0);
    let s = match r {
        Value::List(l) => l
            .borrow()
            .iter()
            .map(|v| v.display())
            .collect::<Vec<_>>()
            .join(&sep),
        _ => String::new(),
    };
    Ok(Value::Str(itrans(s)))
}
fn map_len(r: &Value, _: &mut VM, _: Vec<Value>) -> VResult<Value> {
    Ok(Value::Int(BigInt::from(match r {
        Value::Map(m) => m.borrow().len(),
        _ => 0,
    })))
}
fn map_get(r: &Value, _: &mut VM, a: Vec<Value>) -> VResult<Value> {
    if let Value::Map(m) = r {
        return Ok(m
            .borrow()
            .iter()
            .find(|(k, _)| value_eq(k, a.first().unwrap_or(&Value::Null)))
            .map(|(_, v)| v.clone())
            .unwrap_or(Value::Null));
    }
    Ok(Value::Null)
}
fn map_set(r: &Value, _: &mut VM, a: Vec<Value>) -> VResult<Value> {
    if let Value::Map(m) = r {
        let k = a.first().cloned().unwrap_or(Value::Null);
        let v = a.get(1).cloned().unwrap_or(Value::Null);
        let mut m = m.borrow_mut();
        if let Some(p) = m.iter_mut().find(|(x, _)| value_eq(x, &k)) {
            p.1 = v;
        } else {
            m.push((k, v));
        }
    }
    Ok(Value::Null)
}
fn chan_send(r: &Value, _: &mut VM, a: Vec<Value>) -> VResult<Value> {
    if let Value::Channel(c, _) = r {
        c.borrow_mut()
            .messages
            .push_back(a.first().cloned().unwrap_or(Value::Null));
        Ok(Value::Null)
    } else {
        Err(Fault::new("E0008", "expected channel"))
    }
}
fn chan_recv(r: &Value, _: &mut VM, _: Vec<Value>) -> VResult<Value> {
    if let Value::Channel(c, _) = r {
        c.borrow_mut()
            .messages
            .pop_front()
            .ok_or_else(|| Fault::new("E0011", "task blocked on empty channel"))
    } else {
        Err(Fault::new("E0008", "expected channel"))
    }
}
fn map_keys(r: &Value, _: &mut VM, _: Vec<Value>) -> VResult<Value> {
    let v = match r {
        Value::Map(m) => m.borrow().iter().map(|(k, _)| k.clone()).collect(),
        _ => Vec::new(),
    };
    Ok(Value::List(Rc::new(RefCell::new(v))))
}
