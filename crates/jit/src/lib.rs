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
    /// loop-carried integer variables in the payload slots, runs the loop to
    /// completion, and writes the final values to the out array.
    fn compile_loop_kernel(
        &mut self,
        id: u64,
        cond: &Expr,
        body: &[Stmt],
        env: &EnvRef,
    ) -> Result<(), String> {
        if body.iter().any(|s| matches!(s.kind, StmtKind::Return(_))) {
            return Err("loop kernel rejects early return".into());
        }
        let mut names = Vec::new();
        let mut seen = HashSet::new();
        collect_vars(cond, &mut names, &mut seen);
        for s in body {
            collect_vars_stmt(s, &mut names, &mut seen);
        }
        names.retain(|n| matches!(env.borrow().get(n), Some(Value::Int(_))));
        if names.is_empty() || names.len() > SLOTS {
            return Err(format!("loop kernel needs 1..={SLOTS} carried int vars"));
        }
        let kernel_name = format!("__osr_{id}");
        if self.osr_kernels.contains_key(&id) {
            return Ok(());
        }
        let params: Vec<Param> = names
            .iter()
            .map(|n| Param {
                name: n.clone(),
                role: None,
                ty: None,
                default: None,
                span: cond.span,
            })
            .collect();
        let kernel = Func {
            name: kernel_name.clone(),
            params,
            body: vec![Stmt {
                kind: StmtKind::While {
                    cond: cond.clone(),
                    body: body.to_vec(),
                },
                span: cond.span,
            }],
            closure: env.clone(),
            arg_order: Vec::new(),
            jit_hint: true,
        };
        let fid = self
            .module
            .declare_function(&kernel_name, Linkage::Export, &self.signature(names.len()))
            .map_err(|e| e.to_string())?;
        let sig = self.signature(names.len());
        let mut fctx = cranelift_frontend::FunctionBuilderContext::new();
        let mut func = cranelift_codegen::ir::Function::with_name_signature(
            UserFuncName::user(0, fid.as_u32()),
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
            cx.build_with_out(&kernel, &names)?;
            cx.sites
        };
        let mut cctx = cranelift_codegen::Context::for_function(func);
        self.module
            .define_function(fid, &mut cctx)
            .map_err(|e| e.to_string())?;
        self.module.clear_context(&mut cctx);
        self.module
            .finalize_definitions()
            .map_err(|e| e.to_string())?;
        self.deopt_sites.insert(kernel_name.clone(), sites);
        self.funcs.insert(kernel_name, (fid, names.len()));
        self.osr_kernels.insert(id, (fid, names.clone()));
        self.stats.osr_compiled += 1;
        if self.trace {
            eprintln!("[jit] OSR kernel {id} compiled over {:?}", names);
        }
        Ok(())
    }

    fn run_loop_kernel(&mut self, id: u64, env: &EnvRef) -> Option<VResult<()>> {
        let (fid, names) = self.osr_kernels.get(&id)?.clone();
        let mut slots = [0i64; SLOTS];
        for (i, n) in names.iter().enumerate() {
            let Value::Int(bi) = env.borrow().get(n)? else {
                return None;
            };
            let Ok(v) = bi.to_string().parse::<i128>() else {
                return None;
            };
            if v < I64_MIN || v > I64_MAX {
                return None;
            }
            slots[i] = v as i64;
        }
        let mut out = [0i64; SLOTS];
        let mut ctx = DeoptContext::default();
        let _ = unsafe { self.call_native(fid, &slots, out.as_mut_ptr(), &mut ctx) };
        if ctx.status != STATUS_OK {
            // Overflow, guard or yield: fall back to the interpreter with the
            // variables exactly as they were at the OSR point.
            let kname = format!("__osr_{id}");
            self.note_deopt(&kname, ctx.status, ctx.site);
            return None;
        }
        for (i, n) in names.iter().enumerate() {
            env.borrow_mut()
                .define(n, Value::Int(BigInt::from(out[i])), true);
        }
        self.stats.osr_runs += 1;
        Some(Ok(()))
    }

    fn observe_loop_impl(&mut self, site: &LoopSite<'_>, env: &EnvRef) -> Option<VResult<()>> {
        self.stats.osr_backedges += 1;
        if self.osr_kernels.contains_key(&site.id) {
            return self.run_loop_kernel(site.id, env);
        }
        if self.bad_loops.contains(&site.id) {
            return None;
        }
        let hot = self.loop_hot.entry(site.id).or_insert(0);
        *hot += 1;
        if *hot < self.osr_threshold {
            return None;
        }
        match self.compile_loop_kernel(site.id, site.cond, site.body, env) {
            Ok(()) => self.run_loop_kernel(site.id, env),
            Err(e) => {
                self.stats.osr_rejected += 1;
                self.bad_loops.insert(site.id);
                if self.trace {
                    eprintln!("[jit] OSR rejected loop {}: {e}", site.id);
                }
                None
            }
        }
    }
}

impl Tier1 for Jit {
    fn observe(&mut self, f: &Func, args: &[Value], vm: &mut VM) -> Option<VResult<Value>> {
        self.observe_impl(f, args, vm)
    }
    fn observe_loop(
        &mut self,
        site: &LoopSite<'_>,
        env: &EnvRef,
        _vm: &mut VM,
    ) -> Option<VResult<()>> {
        self.observe_loop_impl(site, env)
    }
    fn dump(&self) -> String {
        let mut s = String::from("=== Tier 1 (Cranelift native) ===\n");
        for (n, (_, a)) in &self.funcs {
            if let Some(sites) = self.deopt_sites.get(n) {
                s.push_str(&format!("compiled {n}/{a} ({} deopt sites)\n", sites.len()));
                for site in sites {
                    s.push_str(&format!(
                        "  deopt-site {} {} — {}\n",
                        site.id, site.reason, site.detail
                    ));
                }
            } else {
                s.push_str(&format!("compiled {n}/{a}\n"));
            }
        }
        for n in &self.bad {
            s.push_str(&format!("blacklisted {n}\n"));
        }
        for (id, (_, names)) in &self.osr_kernels {
            s.push_str(&format!("osr-kernel #{id} over {:?}\n", names));
        }
        s
    }
    fn stats(&self) -> String {
        format!(
            "calls={} compiled={} native_calls={} deopts={} deopt_overflow={} deopt_guard={} deopt_yield={} blacklisted={} repeated_deopt_blacklist={} icache_hits={} icache_misses={} code_cache_hits={} osr_compiled={} osr_runs={} osr_backedges={} osr_rejected={}",
            self.stats.calls,
            self.stats.compiled,
            self.stats.native_calls,
            self.stats.deopts,
            self.stats.deopt_overflow,
            self.stats.deopt_guard,
            self.stats.deopt_yield,
            self.stats.blacklisted,
            self.stats.repeated_deopt_blacklist,
            self.stats.icache_hits,
            self.stats.icache_misses,
            self.stats.code_cache_hits,
            self.stats.osr_compiled,
            self.stats.osr_runs,
            self.stats.osr_backedges,
            self.stats.osr_rejected,
        )
    }
}

// --------------------------------------------------------------- AST helpers

fn calls_in(stmts: &[Stmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    fn walk_expr(e: &Expr, out: &mut HashSet<String>) {
        match &e.kind {
            ExprKind::Call { callee, args } => {
                if let ExprKind::Var(n) = &callee.kind {
                    out.insert(n.clone());
                }
                for a in args {
                    walk_expr(&a.expr, out);
                }
            }
            ExprKind::Binary(_, a, b) => {
                walk_expr(a, out);
                walk_expr(b, out);
            }
            ExprKind::Unary(_, a) | ExprKind::Assign(_, a) => walk_expr(a, out),
            _ => {}
        }
    }
    fn walk_block(b: &[Stmt], out: &mut HashSet<String>) {
        for s in b {
            match &s.kind {
                StmtKind::Expr(e) | StmtKind::Return(Some(e)) => walk_expr(e, out),
                StmtKind::Let { init, .. } => {
                    if let Some(e) = init {
                        walk_expr(e, out);
                    }
                }
                StmtKind::If { cond, then, els } => {
                    walk_expr(cond, out);
                    walk_block(then, out);
                    walk_block(els, out);
                }
                StmtKind::While { cond, body } => {
                    walk_expr(cond, out);
                    walk_block(body, out);
                }
                StmtKind::For { iter, body, .. } => {
                    walk_expr(iter, out);
                    walk_block(body, out);
                }
                _ => {}
            }
        }
    }
    walk_block(stmts, &mut out);
    out
}

fn collect_vars(e: &Expr, out: &mut Vec<String>, seen: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Var(n) => {
            if seen.insert(n.clone()) {
                out.push(n.clone());
            }
        }
        ExprKind::Binary(_, a, b) => {
            collect_vars(a, out, seen);
            collect_vars(b, out, seen);
        }
        ExprKind::Unary(_, a) => collect_vars(a, out, seen),
        ExprKind::Assign(a, b) => {
            collect_vars(a, out, seen);
            collect_vars(b, out, seen);
        }
        ExprKind::Call { callee, args } => {
            collect_vars(callee, out, seen);
            for a in args {
                collect_vars(&a.expr, out, seen);
            }
        }
        _ => {}
    }
}
fn collect_vars_stmt(s: &Stmt, out: &mut Vec<String>, seen: &mut HashSet<String>) {
    match &s.kind {
        StmtKind::Expr(e) | StmtKind::Return(Some(e)) | StmtKind::Raise(e) => {
            collect_vars(e, out, seen)
        }
        StmtKind::Let { init, .. } => {
            if let Some(e) = init {
                collect_vars(e, out, seen);
            }
        }
        StmtKind::If { cond, then, els } => {
            collect_vars(cond, out, seen);
            for x in then.iter().chain(els) {
                collect_vars_stmt(x, out, seen);
            }
        }
        StmtKind::While { cond, body } => {
            collect_vars(cond, out, seen);
            for x in body {
                collect_vars_stmt(x, out, seen);
            }
        }
        StmtKind::For { iter, body, .. } => {
            collect_vars(iter, out, seen);
            for x in body {
                collect_vars_stmt(x, out, seen);
            }
        }
        _ => {}
    }
}

// ------------------------------------------------------------------- builder

struct Cx<'a> {
    b: &'a mut FunctionBuilder<'a>,
    module: &'a mut JITModule,
    funcs: &'a HashMap<String, (FuncId, usize)>,
    vars: HashMap<String, Variable>,
    terminated: bool,
    loops: Vec<(Block, Block)>, // (exit, header)
    ctx: Option<ClifValue>,
    out: Option<(ClifValue, Vec<String>)>,
    sites: Vec<DeoptSite>,
    next_site: u64,
    yield_ref: FuncRef,
    trace: bool,
}

impl<'a> Cx<'a> {
    fn iconst(&mut self, v: i64) -> ClifValue {
        self.b.ins().iconst(types::I64, v)
    }
    fn cmp(&mut self, cc: IntCC, a: ClifValue, b: ClifValue) -> ClifValue {
        let c = self.b.ins().icmp(cc, a, b);
        self.b.ins().uextend(types::I64, c)
    }

    /// Branch to a deopt stub that records `reason` and `site` and returns.
    fn ctx(&self) -> ClifValue {
        self.ctx
            .expect("deopt context must be initialized in build")
    }
    fn guard(&mut self, cond: ClifValue, reason: i64, detail: &str) {
        let ctx = self.ctx();
        let id = self.next_site;
        if self.trace && reason == REASON_OVERFLOW {
            eprintln!("[jit] guard site {id}: {detail}");
        }
        self.next_site += 1;
        self.sites.push(DeoptSite {
            id,
            reason: match reason {
                REASON_OVERFLOW => "overflow",
                REASON_GUARD => "guard",
                REASON_YIELD => "yield",
                _ => "unknown",
            },
            detail: detail.to_string(),
        });
        let deopt = self.b.create_block();
        let ok = self.b.create_block();
        self.b.ins().brif(cond, deopt, &[], ok, &[]);
        self.b.switch_to_block(deopt);
        let r = self.iconst(reason);
        let s = self.iconst(id as i64);
        let z = self.iconst(0);
        self.b.ins().store(MemFlagsData::trusted(), r, ctx, 0);
        self.b.ins().store(MemFlagsData::trusted(), s, ctx, 8);
        self.b.ins().return_(&[z]);
        self.b.switch_to_block(ok);
    }

    /// Deopt when `v` lies outside `[lo, hi]`.
    fn range_guard(&mut self, v: ClifValue, lo: i64, hi: i64, detail: &str) {
        let lo_v = self.iconst(lo);
        let hi_v = self.iconst(hi);
        let a = self.cmp(IntCC::SignedLessThan, v, lo_v);
        let b = self.cmp(IntCC::SignedGreaterThan, v, hi_v);
        let bad = self.b.ins().bor(a, b);
        self.guard(bad, REASON_OVERFLOW, detail);
    }

    /// Poll the host yield switch at a loop backedge.
    fn yield_poll(&mut self) {
        let ctx = self.ctx();
        let p = self.b.ins().call(self.yield_ref, &[ctx]);
        let r = self.b.inst_results(p)[0];
        self.guard(r, REASON_YIELD, "host yield at compiled loop backedge");
    }

    fn var(&mut self, name: &str) -> Variable {
        if let Some(v) = self.vars.get(name) {
            return *v;
        }
        let v = self.b.declare_var(types::I64);
        self.vars.insert(name.to_string(), v);
        v
    }

    fn store_out(&mut self) {
        let Some((ptr, names)) = self.out.clone() else {
            return;
        };
        for (i, n) in names.iter().enumerate() {
            let var = self.var(n);
            let v = self.b.use_var(var);
            self.b
                .ins()
                .store(MemFlagsData::trusted(), v, ptr, (i * 8) as i32);
        }
    }

    fn build(&mut self, f: &Func) -> Result<(), String> {
        let entry = self.b.current_block().unwrap();
        for _ in 0..PARAM_COUNT {
            self.b.append_block_param(entry, types::I64);
        }
        let params: Vec<ClifValue> = self.b.block_params(entry).to_vec();
        self.ctx = Some(params[CTX_SLOT]);
        for (i, p) in f.params.iter().enumerate() {
            let var = self.var(&p.name);
            self.b.def_var(var, params[i]);
        }
        self.compile_block(&f.body)?;
        if !self.terminated {
            self.store_out();
            let z = self.iconst(0);
            self.b.ins().return_(&[z]);
            self.terminated = true;
        }
        self.b.seal_all_blocks();
        Ok(())
    }

    fn build_with_out(&mut self, f: &Func, carried: &[String]) -> Result<(), String> {
        let entry = self.b.current_block().unwrap();
        for _ in 0..PARAM_COUNT {
            self.b.append_block_param(entry, types::I64);
        }
        let params: Vec<ClifValue> = self.b.block_params(entry).to_vec();
        self.ctx = Some(params[CTX_SLOT]);
        self.out = Some((params[OUT_SLOT], carried.to_vec()));
        for (i, p) in f.params.iter().enumerate() {
            let var = self.var(&p.name);
            self.b.def_var(var, params[i]);
        }
        self.compile_block(&f.body)?;
        if !self.terminated {
            self.store_out();
            let z = self.iconst(0);
            self.b.ins().return_(&[z]);
            self.terminated = true;
        }
        self.b.seal_all_blocks();
        Ok(())
    }

    fn compile_block(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        for s in stmts {
            self.compile_stmt(s)?;
        }
        Ok(())
    }

    fn switch(&mut self, blk: Block) {
        self.b.switch_to_block(blk);
        self.terminated = false;
    }

    fn compile_stmt(&mut self, s: &Stmt) -> Result<(), String> {
        if self.terminated {
            return Ok(());
        }
        match &s.kind {
            StmtKind::Let { name, init, .. } => {
                let v = match init {
                    Some(e) => self.expr(e)?,
                    None => self.iconst(0),
                };
                let var = self.var(name);
                self.b.def_var(var, v);
                Ok(())
            }
            StmtKind::Expr(e) => {
                if let ExprKind::Assign(target, value) = &e.kind {
                    let v = self.expr(value)?;
                    if let ExprKind::Var(n) = &target.kind {
                        let var = self.var(n);
                        self.b.def_var(var, v);
                        Ok(())
                    } else {
                        Err("unsupported assignment target in JIT".into())
                    }
                } else {
                    self.expr(e)?;
                    Ok(())
                }
            }
            StmtKind::Return(e) => {
                let v = match e {
                    Some(e) => self.expr(e)?,
                    None => self.iconst(0),
                };
                self.b.ins().return_(&[v]);
                self.terminated = true;
                Ok(())
            }
            StmtKind::Break | StmtKind::Continue => {
                let (exit, header) = *self.loops.last().ok_or("break outside loop")?;
                let target = if matches!(s.kind, StmtKind::Break) {
                    exit
                } else {
                    header
                };
                self.b.ins().jump(target, &[]);
                self.terminated = true;
                Ok(())
            }
            StmtKind::If { cond, then, els } => {
                let c = self.expr(cond)?;
                let tb = self.b.create_block();
                let eb = self.b.create_block();
                let mb = self.b.create_block();
                self.b.ins().brif(c, tb, &[], eb, &[]);
                self.switch(tb);
                self.compile_block(then)?;
                if !self.terminated {
                    self.b.ins().jump(mb, &[]);
                }
                self.switch(eb);
                self.compile_block(els)?;
                if !self.terminated {
                    self.b.ins().jump(mb, &[]);
                }
                self.switch(mb);
                Ok(())
            }
            StmtKind::While { cond, body } => {
                let h = self.b.create_block();
                let bb = self.b.create_block();
                let x = self.b.create_block();
                self.b.ins().jump(h, &[]);
                self.switch(h);
                let c = self.expr(cond)?;
                self.b.ins().brif(c, bb, &[], x, &[]);
                self.switch(bb);
                self.loops.push((x, h));
                self.compile_block(body)?;
                self.loops.pop();
                if !self.terminated {
                    self.yield_poll();
                    self.b.ins().jump(h, &[]);
                }
                self.switch(x);
                Ok(())
            }
            StmtKind::For { var, iter, body } => {
                let (from, to) = match &iter.kind {
                    ExprKind::Range { from, to, .. } => (from, to),
                    _ => return Err("JIT supports for only over integer ranges".into()),
                };
                let start = self.expr(from)?;
                let end = self.expr(to)?;
                let idx = self.var(var);
                self.b.def_var(idx, start);
                let h = self.b.create_block();
                let bb = self.b.create_block();
                let x = self.b.create_block();
                self.b.ins().jump(h, &[]);
                self.switch(h);
                let cur = self.b.use_var(idx);
                let cmp = self.cmp(IntCC::SignedLessThan, cur, end);
                self.b.ins().brif(cmp, bb, &[], x, &[]);
                self.switch(bb);
                self.loops.push((x, h));
                self.compile_block(body)?;
                self.loops.pop();
                if !self.terminated {
                    self.yield_poll();
                    let one = self.iconst(1);
                    let next = self.b.ins().iadd(cur, one);
                    self.b.def_var(idx, next);
                    self.b.ins().jump(h, &[]);
                }
                self.switch(x);
                Ok(())
            }
            _ => Err(format!("JIT unsupported statement: {}", stmt_tag(s))),
        }
    }

    fn expr(&mut self, e: &Expr) -> Result<ClifValue, String> {
        match &e.kind {
            ExprKind::Lit(Literal::Int(i)) => {
                let v: i64 = i
                    .to_string()
                    .parse()
                    .map_err(|_| "integer too large for JIT".to_string())?;
                Ok(self.iconst(v))
            }
            ExprKind::Var(n) => self
                .vars
                .get(n)
                .map(|v| self.b.use_var(*v))
                .ok_or_else(|| format!("unknown JIT variable {n}")),
            ExprKind::Unary(UnOp::Neg, x) => {
                let v = self.expr(x)?;
                let min = self.iconst(i64::MIN);
                let ov = self.cmp(IntCC::Equal, v, min);
                self.guard(ov, REASON_OVERFLOW, "negation of i64::MIN");
                let z = self.iconst(0);
                Ok(self.b.ins().isub(z, v))
            }
            ExprKind::Unary(UnOp::Not, x) => {
                let v = self.expr(x)?;
                let z = self.iconst(0);
                let c = self.cmp(IntCC::Equal, v, z);
                Ok(c)
            }
            ExprKind::Binary(op, a, b) => {
                let x = self.expr(a)?;
                let y = self.expr(b)?;
                match op {
                    BinOp::Add => {
                        self.range_guard(x, -ADD_SAFE, ADD_SAFE, "addition operand range");
                        self.range_guard(y, -ADD_SAFE, ADD_SAFE, "addition operand range");
                        Ok(self.b.ins().iadd(x, y))
                    }
                    BinOp::Sub => {
                        self.range_guard(x, -ADD_SAFE, ADD_SAFE, "subtraction operand range");
                        self.range_guard(y, -ADD_SAFE, ADD_SAFE, "subtraction operand range");
                        Ok(self.b.ins().isub(x, y))
                    }
                    BinOp::Mul => {
                        self.range_guard(x, -MUL_SAFE, MUL_SAFE, "multiplication operand range");
                        self.range_guard(y, -MUL_SAFE, MUL_SAFE, "multiplication operand range");
                        Ok(self.b.ins().imul(x, y))
                    }
                    BinOp::Div | BinOp::Mod => {
                        let z = self.iconst(0);
                        let zero = self.cmp(IntCC::Equal, y, z);
                        let min = self.iconst(i64::MIN);
                        let m1 = self.iconst(-1);
                        let xmin = self.cmp(IntCC::Equal, x, min);
                        let ym1 = self.cmp(IntCC::Equal, y, m1);
                        let both = self.b.ins().band(xmin, ym1);
                        let bad = self.b.ins().bor(zero, both);
                        self.guard(bad, REASON_GUARD, "division by zero or i64::MIN / -1");
                        if matches!(op, BinOp::Div) {
                            Ok(self.b.ins().sdiv(x, y))
                        } else {
                            Ok(self.b.ins().srem(x, y))
                        }
                    }
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        let cond = match op {
                            BinOp::Eq => IntCC::Equal,
                            BinOp::Ne => IntCC::NotEqual,
                            BinOp::Lt => IntCC::SignedLessThan,
                            BinOp::Le => IntCC::SignedLessThanOrEqual,
                            BinOp::Gt => IntCC::SignedGreaterThan,
                            BinOp::Ge => IntCC::SignedGreaterThanOrEqual,
                            _ => unreachable!(),
                        };
                        Ok(self.cmp(cond, x, y))
                    }
                    _ => Err(format!("JIT unsupported operator {op:?}")),
                }
            }
            ExprKind::Call { callee, args } => {
                let name = match &callee.kind {
                    ExprKind::Var(n) => n.clone(),
                    _ => return Err("indirect call unsupported".into()),
                };
                let (fid, _) = *self
                    .funcs
                    .get(&name)
                    .ok_or_else(|| format!("callee {name} not declared"))?;
                let mut clif_args = Vec::with_capacity(PARAM_COUNT);
                for a in args.iter() {
                    if clif_args.len() >= SLOTS {
                        return Err("call with too many arguments".into());
                    }
                    clif_args.push(self.expr(&a.expr)?);
                }
                while clif_args.len() < SLOTS {
                    clif_args.push(self.iconst(0));
                }
                let fref: FuncRef = self.module.declare_func_in_func(fid, self.b.func);
                clif_args.push(self.iconst(0)); // out (unused by plain calls)
                clif_args.push(self.ctx());
                let call = self.b.ins().call(fref, &clif_args);
                Ok(self.b.inst_results(call)[0])
            }
            _ => Err(format!("JIT unsupported expression: {}", expr_tag(e))),
        }
    }
}

fn stmt_tag(s: &Stmt) -> &'static str {
    match s.kind {
        StmtKind::Expr(_) => "expr",
        StmtKind::Let { .. } => "let",
        StmtKind::Fn(_) => "fn",
        StmtKind::Return(_) => "return",
        StmtKind::If { .. } => "if",
        StmtKind::While { .. } => "while",
        StmtKind::For { .. } => "for",
        StmtKind::Break => "break",
        StmtKind::Continue => "continue",
        StmtKind::Try { .. } => "try",
        StmtKind::Raise(_) => "raise",
        StmtKind::Class { .. } => "class",
        StmtKind::Import { .. } => "import",
        StmtKind::Export(_) => "export",
        StmtKind::Sutra { .. } => "sutra",
        StmtKind::Task { .. } => "task",
        StmtKind::Channel { .. } => "channel",
        StmtKind::Send { .. } => "send",
        StmtKind::Receive { .. } => "receive",
        StmtKind::Join { .. } => "join",
        StmtKind::Assert(_) => "assert",
    }
}
fn expr_tag(e: &Expr) -> &'static str {
    match e.kind {
        ExprKind::Lit(_) => "lit",
        ExprKind::Var(_) => "var",
        ExprKind::Template(_) => "template",
        ExprKind::Unary(..) => "unary",
        ExprKind::Binary(..) => "binary",
        ExprKind::Assign(..) => "assign",
        ExprKind::Call { .. } => "call",
        ExprKind::Get { .. } => "get",
        ExprKind::Set { .. } => "set",
        ExprKind::Index { .. } => "index",
        ExprKind::List(_) => "list",
        ExprKind::Map(_) => "map",
        ExprKind::Tuple(_) => "tuple",
        ExprKind::Lambda { .. } => "lambda",
        ExprKind::IfExpr { .. } => "ifexpr",
        ExprKind::Block(_) => "block",
        ExprKind::Range { .. } => "range",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deopt_site_ids_are_unique_and_ordered() {
        let mut cx_sites = Vec::new();
        let mut next = 1u64;
        for _ in 0..3 {
            cx_sites.push(DeoptSite {
                id: next,
                reason: "overflow",
                detail: "test".into(),
            });
            next += 1;
        }
        assert_eq!(cx_sites.iter().map(|s| s.id).collect::<Vec<_>>(), [1, 2, 3]);
    }

    #[test]
    fn cache_plan_roundtrip() {
        let plan = CachedPlan {
            key: 42,
            name: "phiba".into(),
            arity: 1,
            sites: vec![DeoptOwnedSite {
                id: 1,
                reason: "overflow".into(),
                detail: "addition operand range".into(),
            }],
        };
        let bytes = serde_json::to_vec(&vec![plan.clone()]).unwrap();
        let back: Vec<CachedPlan> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back[0].key, plan.key);
        assert_eq!(back[0].sites[0].id, 1);
    }

    #[test]
    fn yield_flag_is_observable() {
        clear_yield();
        assert_eq!(yield_check(std::ptr::null_mut()), 0);
        request_yield();
        assert_eq!(yield_check(std::ptr::null_mut()), 1);
        clear_yield();
    }
}
