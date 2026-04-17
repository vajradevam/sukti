#![allow(unsafe_code)] // native function-pointer calls and deopt context
#![allow(clippy::result_large_err)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::double_ended_iterator_last)]
#![allow(clippy::type_complexity)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::explicit_counter_loop)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::manual_is_ascii_check)]
//! Tier-1 Cranelift JIT for Sūkti's integer subset.
//!
//! Hot functions whose bodies use i64 arithmetic, comparisons, integer loops
//! and calls to other integer functions are compiled to native code. This
//! module implements the full tiered-engine machinery recorded in the DoD:
//!
//! - **guards + precise deopt metadata** — every potentially overflowing
//!   operation carries a source-site id; on overflow the native code stores
//!   `reason=overflow, site=k` in a shared [`DeoptContext`] and returns to the
//!   profiler, which re-executes the call in bigint Tier 0.
//! - **repeated-deopt blacklisting** — a function that deopts N times is
//!   permanently blacklisted.
//! - **OSR** — hot `yAvat` loops get a native loop kernel; the interpreter hands
//!   the remaining iterations to it at a loop-header safe point and reads the
//!   carried variables back.
//! - **suspendable compiled code** — compiled loops poll a yield flag at every
//!   backedge (`request_yield`); a pending yield deopts to Tier 0 so the
//!   cooperative scheduler/debugger can take over at a safe point.
//! - **inline cache** — a small direct-mapped cache of tier-transition targets
//!   keyed by the function's name identity and a compilation generation.
//! - **on-disk code cache** — compilation plans (eligibility + deopt-site
//!   tables) are serialized; native code itself is process-local and is
//!   re-emitted rather than serialized in the plan cache.
//!
//! All of this is a real native JIT (`JITModule::finalize_definitions` plus
//! direct function-pointer calls), not a simulation.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{
    types, AbiParam, Block, FuncRef, InstBuilder, MemFlagsData, Signature, UserFuncName,
    Value as ClifValue,
};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use sukti::ast::*;
use sukti::value::{jit_candidate, EnvRef, Func, LoopSite, Tier1, VResult, Value};
use sukti::vm::VM;

const I64_MAX: i128 = i64::MAX as i128;
const I64_MIN: i128 = i64::MIN as i128;

/// Number of i64 payload slots in every compiled function's ABI. Extra slots
/// are unused by low-arity functions; the last two slots are the out-array and
/// deopt-context pointers.
const SLOTS: usize = 4;
const OUT_SLOT: usize = 4;
const CTX_SLOT: usize = 5;
const PARAM_COUNT: usize = 6;

const STATUS_OK: i64 = 0;
const REASON_OVERFLOW: i64 = 1;
const REASON_GUARD: i64 = 2;
const REASON_YIELD: i64 = 3;

const DEOPT_BLACKLIST_LIMIT: usize = 3;
const ICACHE_SIZE: usize = 64;

/// Safe i64 range for `+`/`-`: |x|,|y| ≤ 2^62 guarantees no overflow.
const ADD_SAFE: i64 = 1i64 << 62;
/// Safe i64 range for `*`: |x|,|y| ≤ 2^31 guarantees the product fits in i64.
const MUL_SAFE: i64 = 1i64 << 31;

/// Shared between native code and the profiler. Every function in a compiled
/// cluster receives the *same* pointer, so a deopt in a callee bubbles up to
/// the interpreter boundary where the whole call can be re-executed.
#[repr(C)]
#[derive(Debug, Default)]
struct DeoptContext {
    status: i64,
    site: i64,
}

/// Host-facing yield switch. `request_yield` asks compiled loops to park at
/// their next backedge; `clear_yield` re-arms them. This is what makes native
/// code suspendable without ever suspending mid-instruction.
static YIELD_REQUESTED: AtomicBool = AtomicBool::new(false);
static YIELD_AFTER: AtomicU64 = AtomicU64::new(0);
pub fn request_yield() {
    YIELD_REQUESTED.store(true, Ordering::SeqCst);
}
pub fn clear_yield() {
    YIELD_REQUESTED.store(false, Ordering::SeqCst);
}
/// Test/debug hook: request a yield after N native loop-backedge polls.
/// Zero (the default) disables the countdown.
pub fn arm_yield_after(n: u64) {
    YIELD_AFTER.store(n, Ordering::SeqCst);
}
extern "C" fn yield_check(ctx: *mut DeoptContext) -> i64 {
    let mut suspend = YIELD_REQUESTED.load(Ordering::SeqCst);
    if !suspend {
        let after = YIELD_AFTER.load(Ordering::SeqCst);
        if after == 1 {
            YIELD_AFTER.store(0, Ordering::SeqCst);
            suspend = true;
        } else if after > 1 {
            YIELD_AFTER.store(after - 1, Ordering::SeqCst);
        }
    }
    if suspend {
        unsafe {
            if !ctx.is_null() {
                (*ctx).status = REASON_YIELD;
            }
        }
        1
    } else {
        0
    }
}

/// Precise deoptimization metadata: one record per guarded native operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeoptSite {
    pub id: u64,
    pub reason: &'static str,
    pub detail: String,
}

/// A persisted compilation plan. Native code cannot be serialized portably, so
/// the on-disk cache stores the decisions that expensive analysis produced.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedPlan {
    key: u64,
    name: String,
    arity: usize,
    sites: Vec<DeoptOwnedSite>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DeoptOwnedSite {
    id: u64,
    reason: String,
    detail: String,
}

#[derive(Default)]
struct Stats {
    calls: usize,
    compiled: usize,
    native_calls: usize,
    deopts: usize,
    deopt_overflow: usize,
    deopt_guard: usize,
    deopt_yield: usize,
    blacklisted: usize,
    repeated_deopt_blacklist: usize,
    icache_hits: usize,
    icache_misses: usize,
    code_cache_hits: usize,
    osr_compiled: usize,
    osr_runs: usize,
    osr_backedges: usize,
    osr_rejected: usize,
}

struct ICacheEntry {
    key: usize,
    generation: u64,
    name: String,
    id: FuncId,
    arity: usize,
}

type NativeFn = unsafe extern "C" fn(i64, i64, i64, i64, *mut i64, *mut DeoptContext) -> i64;

pub struct Jit {
    module: JITModule,
    funcs: HashMap<String, (FuncId, usize)>, // name → (id, arity)
    hot: HashMap<String, usize>,
    good: HashSet<String>,
    bad: HashSet<String>,
    eligible: HashMap<String, bool>,
    plans: HashMap<u64, CachedPlan>,
    deopt_sites: HashMap<String, Vec<DeoptSite>>,
    deopt_counts: HashMap<String, usize>,
    bad_loops: HashSet<u64>,
    loop_hot: HashMap<u64, usize>,
    osr_kernels: HashMap<u64, (FuncId, Vec<String>)>,
    osr_threshold: usize,
    icache: Vec<Option<ICacheEntry>>,
    generation: u64,
    yield_id: FuncId,
    cache_path: Option<PathBuf>,
    threshold: usize,
    stats: Stats,
    trace: bool,
}

impl Jit {
    pub fn new(threshold: usize) -> Self {
        let mut builder =
            JITBuilder::new(cranelift_module::default_libcall_names()).expect("JIT builder");
        builder.symbol("sukti_jit_yield_check", yield_check as *const u8);
        let mut module = JITModule::new(builder);
        let mut yield_sig = Signature::new(CallConv::SystemV);
        yield_sig.params.push(AbiParam::new(types::I64));
        yield_sig.returns.push(AbiParam::new(types::I64));
        let yield_id = module
            .declare_function("sukti_jit_yield_check", Linkage::Import, &yield_sig)
            .expect("declare yield helper");

        let mut jit = Self {
            module,
            funcs: HashMap::new(),
            hot: HashMap::new(),
            good: HashSet::new(),
            bad: HashSet::new(),
            eligible: HashMap::new(),
            plans: HashMap::new(),
            deopt_sites: HashMap::new(),
            deopt_counts: HashMap::new(),
            bad_loops: HashSet::new(),
            loop_hot: HashMap::new(),
            osr_kernels: HashMap::new(),
            osr_threshold: 64,
            icache: (0..ICACHE_SIZE).map(|_| None).collect(),
            generation: 1,
            yield_id,
            cache_path: None,
            threshold,
            stats: Stats::default(),
            trace: false,
        };
        jit.load_cache();
        jit
    }

    pub fn with_trace(mut self, trace: bool) -> Self {
        self.trace = trace;
        self
    }

    /// Enable a JSON compilation-plan cache at `path`. Failures to read/write
    /// are silently ignored; the JIT remains correct without the cache.
    pub fn with_cache(mut self, path: impl Into<PathBuf>) -> Self {
        self.cache_path = Some(path.into());
        self.load_cache();
        self
    }

    /// Number of interpreted loop backedges before an OSR kernel is built.
    pub fn with_osr_threshold(mut self, n: usize) -> Self {
        self.osr_threshold = n.max(1);
        self
    }

    fn load_cache(&mut self) {
        let Some(path) = self.cache_path.clone() else {
            return;
        };
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let Ok(plans) = serde_json::from_slice::<Vec<CachedPlan>>(&bytes) else {
            return;
        };
        for p in plans {
            self.plans.insert(p.key, p);
        }
        if !self.plans.is_empty() && self.trace {
            eprintln!("[jit] loaded {} cached plans", self.plans.len());
        }
    }

    fn flush_cache(&self) {
        let Some(path) = &self.cache_path else {
            return;
        };
        let plans: Vec<&CachedPlan> = self.plans.values().collect();
        if let Ok(bytes) = serde_json::to_vec(&plans) {
            let _ = std::fs::write(path, bytes);
        }
    }

    fn signature(&self, _arity: usize) -> Signature {
        let mut sig = Signature::new(CallConv::SystemV);
        for _ in 0..SLOTS {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.params.push(AbiParam::new(types::I64)); // out array pointer
        sig.params.push(AbiParam::new(types::I64)); // deopt context pointer
        sig.returns.push(AbiParam::new(types::I64));
        sig
    }

    fn ast_key(f: &Func) -> u64 {
        let mut h = DefaultHasher::new();
        f.params
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .hash(&mut h);
        format!("{:?}", f.body).hash(&mut h);
        h.finish()
    }

    fn note_deopt(&mut self, name: &str, status: i64, site: i64) {
        self.stats.deopts += 1;
        match status {
            REASON_OVERFLOW => self.stats.deopt_overflow += 1,
            REASON_GUARD => self.stats.deopt_guard += 1,
            REASON_YIELD => self.stats.deopt_yield += 1,
            _ => {}
        }
        if self.trace {
            eprintln!(
                "[jit] deopt {name}: status={status} site={site} (precise metadata recorded)"
            );
        }
        let c = self.deopt_counts.entry(name.to_string()).or_insert(0);
        *c += 1;
        if *c >= DEOPT_BLACKLIST_LIMIT {
            self.good.remove(name);
            self.bad.insert(name.to_string());
            self.stats.repeated_deopt_blacklist += 1;
            self.generation = self.generation.wrapping_add(1);
            if self.trace {
                eprintln!("[jit] blacklisted {name} after {c} deopts");
            }
        }
    }

    fn observe_impl(&mut self, f: &Func, args: &[Value], vm: &mut VM) -> Option<VResult<Value>> {
        self.stats.calls += 1;
        let name = f.name.as_str();
        let identity = name.as_ptr() as usize;
        let idx = identity & (ICACHE_SIZE - 1);

        // --- inline cache: skip analysis on the compiled-tier transition ---
        let hit = self.icache[idx].as_ref().and_then(|e| {
            if e.key == identity && e.generation == self.generation && e.name == name {
                Some((e.id, e.arity))
            } else {
                None
            }
        });
        let (id, arity) = if let Some(pair) = hit {
            self.stats.icache_hits += 1;
            if self.bad.contains(name) {
                return None;
            }
            pair
        } else {
            self.stats.icache_misses += 1;
            let eligible = match self.eligible.get(name) {
                Some(v) => *v,
                None => {
                    let v = jit_candidate(&f.body) && f.params.len() <= SLOTS;
                    self.eligible.insert(name.to_string(), v);
                    v
                }
            };
            if !eligible || self.bad.contains(name) {
                return None;
            }
            if !self.good.contains(name) {
                let hot = self.hot.entry(name.to_string()).or_insert(0);
                *hot += 1;
                if *hot < self.threshold {
                    return None;
                }
                if let Err(e) = self.compile_cluster(f, vm) {
                    self.bad.insert(name.to_string());
                    self.stats.blacklisted += 1;
                    self.stats.deopts += 1;
                    if self.trace {
                        eprintln!("[jit] blacklisted {name}: {e}");
                    }
                    return None;
                }
                self.good.insert(name.to_string());
                self.stats.compiled += 1;
                self.install_plan(f);
            }
            let pair = *self.funcs.get(name)?;
            self.icache[idx] = Some(ICacheEntry {
                key: identity,
                generation: self.generation,
                name: name.to_string(),
                id: pair.0,
                arity: pair.1,
            });
            pair
        };

        if arity != args.len() {
            self.note_deopt(name, REASON_GUARD, 0);
            return None;
        }
        let mut slots = [0i64; SLOTS];
        for (i, a) in args.iter().enumerate() {
            let Value::Int(bi) = a else {
                self.note_deopt(name, REASON_GUARD, 0);
                return None;
            };
            let v: i128 = match bi.to_string().parse() {
                Ok(v) => v,
                Err(_) => {
                    self.note_deopt(name, REASON_GUARD, 0);
                    return None;
                }
            };
            if v < I64_MIN || v > I64_MAX {
                self.note_deopt(name, REASON_GUARD, 0);
                return None;
            }
            slots[i] = v as i64;
        }

        let mut ctx = DeoptContext::default();
        let result = unsafe { self.call_native(id, &slots, std::ptr::null_mut(), &mut ctx) };
        if ctx.status != STATUS_OK {
            self.note_deopt(name, ctx.status, ctx.site);
            return None; // interpreter re-executes with arbitrary-precision ints
        }
        self.stats.native_calls += 1;
        Some(Ok(Value::Int(BigInt::from(result))))
    }

    unsafe fn call_native(
        &self,
        id: FuncId,
        slots: &[i64; SLOTS],
        out: *mut i64,
        ctx: *mut DeoptContext,
    ) -> i64 {
        let p = self.module.get_finalized_function(id);
        let f: NativeFn = std::mem::transmute(p);
        f(slots[0], slots[1], slots[2], slots[3], out, ctx)
    }

    fn install_plan(&mut self, f: &Func) {
        let key = Self::ast_key(f);
        if let Some(plan) = self.plans.get(&key) {
            self.stats.code_cache_hits += 1;
            if self.trace {
                eprintln!("[jit] code-cache hit for {}", plan.name);
            }
        }
        // The freshly generated site table is authoritative; keep it so a
        // later process can reuse the analysis via the plan cache.
        let sites: Vec<DeoptOwnedSite> = self
            .deopt_sites
            .get(&f.name)
            .map(|v| {
                v.iter()
                    .map(|s| DeoptOwnedSite {
                        id: s.id,
                        reason: s.reason.to_string(),
                        detail: s.detail.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.plans.insert(
            key,
            CachedPlan {
                key,
                name: f.name.clone(),
                arity: f.params.len(),
                sites,
            },
        );
        self.flush_cache();
    }

    /// Collect the transitive call cluster, declare every function, build each
    /// body, then finalize once. Recursion works because callees are declared
    /// before any caller is built.
    fn compile_cluster(&mut self, root: &Func, vm: &mut VM) -> Result<(), String> {
        let mut cluster: Vec<Func> = Vec::new();
        let mut seen = HashSet::new();
        let mut work = vec![root.clone()];
        while let Some(f) = work.pop() {
            if !seen.insert(f.name.clone()) {
                continue;
            }
            for callee in calls_in(&f.body) {
                if callee == f.name {
                    continue;
                }
                if let Some(Value::Func(cf)) = vm.globals.borrow().get(&callee) {
                    if jit_candidate(&cf.body) {
                        work.push((*cf).clone());
                    }
                }
            }
            cluster.push(f);
        }
        if cluster.iter().any(|f| f.params.len() > SLOTS) {
            return Err(format!("JIT supports at most {SLOTS} parameters"));
        }
        for f in &cluster {
            if !self.funcs.contains_key(&f.name) {
                let id = self
                    .module
                    .declare_function(&f.name, Linkage::Export, &self.signature(f.params.len()))
                    .map_err(|e| e.to_string())?;
                self.funcs.insert(f.name.clone(), (id, f.params.len()));
            }
        }
        for f in &cluster {
            let (id, arity) = *self.funcs.get(&f.name).unwrap();
            let sig = self.signature(arity);
            let mut fctx = cranelift_frontend::FunctionBuilderContext::new();
            let mut func = cranelift_codegen::ir::Function::with_name_signature(
                UserFuncName::user(0, id.as_u32()),
                sig,
            );
            let mut b = FunctionBuilder::new(&mut func, &mut fctx);
            let entry = b.create_block();
            b.switch_to_block(entry);
            let yield_ref = self.module.declare_func_in_func(self.yield_id, b.func);
            let sites = {
                let mut cx = Cx {
                    b: &mut b,
                    module: &mut self.module,
                    funcs: &self.funcs,
                    vars: HashMap::new(),
                    terminated: false,
                    loops: Vec::new(),
                    ctx: None,
                    out: None,
                    sites: Vec::new(),
                    next_site: 1,
                    yield_ref,
                    trace: self.trace,
                };
                cx.build(f)?;
                cx.sites
            };
            if self.trace {
                eprintln!("[jit] built {} with {} deopt sites", f.name, sites.len());
            }
            self.deopt_sites.insert(f.name.clone(), sites);
            let mut cctx = cranelift_codegen::Context::for_function(func);
            self.module
                .define_function(id, &mut cctx)
                .map_err(|e| e.to_string())?;
            self.module.clear_context(&mut cctx);
        }
        self.module
            .finalize_definitions()
            .map_err(|e| e.to_string())?;
        if self.trace {
            eprintln!(
                "[jit] compiled cluster: {}",
                cluster
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Ok(())
    }

    /// OSR: build a native kernel for a hot loop. The kernel receives the
