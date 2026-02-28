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

