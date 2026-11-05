//! Loads sidebar plugins into a sandbox and asks them for a sidebar description.
//!
//! Prototype ABI (see docs/plugin-system/spec.md): the plugin exports `memory`,
//! `alloc(len) -> ptr`, `dealloc(ptr, cap)` and `render(ptr, len) -> ptr`; the reply at
//! the returned pointer is a 4-byte little-endian length followed by JSON.
use std::time::Duration;
use wasmtime::error::{Context, Result, bail, format_err};
use wasmtime::{Config, Instance, Memory, Store, TypedFunc};

/// How long one call may run before it is interrupted, in epoch ticks of `TICK`.
pub const TICK: Duration = Duration::from_millis(10);
const DEADLINE_TICKS: u64 = 5;
/// Linear memory a plugin may grow to.
const MEMORY_LIMIT: usize = 64 << 20;
/// The largest reply taken. A left column is rows of short text: past this it is a plugin
/// misbehaving, and reading it would happen on the UI thread, outside the call's deadline.
pub const REPLY_LIMIT: usize = 1 << 20;

pub fn engine() -> Result<Engine> {
    let mut config = Config::new();
    config.epoch_interruption(true);
    Engine::new(&config)
}

pub use wasmtime::{Engine, Module};

/// Compiles a module from its `.wasm` bytes for `Plugin::new`.
pub fn module(engine: &Engine, wasm: &[u8]) -> Result<Module> {
    Module::new(engine, wasm)
}

/// Compiles once; the bytes can be cached on disk and loaded with `Plugin::from_precompiled`.
pub fn precompile(engine: &Engine, wasm: &[u8]) -> Result<Vec<u8>> {
    engine.precompile_module(wasm)
}

struct Limits(wasmtime::StoreLimits);

pub struct Plugin {
    store: Store<Limits>,
    memory: Memory,
    alloc: TypedFunc<u32, u32>,
    dealloc: TypedFunc<(u32, u32), ()>,
    render: TypedFunc<(u32, u32), u32>,
    instance: Instance,
}

impl Plugin {
    pub fn new(engine: &Engine, module: &Module) -> Result<Self> {
        let limits = wasmtime::StoreLimitsBuilder::new()
            .memory_size(MEMORY_LIMIT)
            .build();
        let mut store = Store::new(engine, Limits(limits));
        store.limiter(|l| &mut l.0);
        store.set_epoch_deadline(DEADLINE_TICKS);
        // No imports: the plugin has no way to reach files, network or processes.
        let instance = Instance::new(&mut store, module, &[])?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .context("plugin exports no memory")?;
        Ok(Self {
            alloc: instance.get_typed_func(&mut store, "alloc")?,
            dealloc: instance.get_typed_func(&mut store, "dealloc")?,
            render: instance.get_typed_func(&mut store, "render")?,
            memory,
            store,
            instance,
        })
    }

    /// # Safety
    /// `bytes` must come from `precompile` with the same engine configuration.
    pub unsafe fn from_precompiled(engine: &Engine, bytes: &[u8]) -> Result<Self> {
        // SAFETY: forwarded to the caller.
        let module = unsafe { Module::deserialize(engine, bytes)? };
        Self::new(engine, &module)
    }

    /// One sidebar update: state JSON in, sidebar JSON out. Any trap, timeout or bad
    /// reply is an error, never a crash; the caller falls back to the built-in view.
    pub fn render(&mut self, state: &[u8]) -> Result<Vec<u8>> {
        self.store.set_epoch_deadline(DEADLINE_TICKS);
        let len = u32::try_from(state.len()).context("state too large")?;
        let input = self.alloc.call(&mut self.store, len)?;
        self.memory.write(&mut self.store, input as usize, state)?;
        let out = self.render.call(&mut self.store, (input, len))?;
        self.dealloc.call(&mut self.store, (input, len))?;
        let mut header = [0u8; 4];
        self.memory.read(&self.store, out as usize, &mut header)?;
        let n = u32::from_le_bytes(header) as usize;
        if n > REPLY_LIMIT {
            bail!("回复太大（{n} 字节，最多 {REPLY_LIMIT}）");
        }
        let mut reply = vec![0u8; n];
        self.memory
            .read(&self.store, out as usize + 4, &mut reply)?;
        self.dealloc.call(&mut self.store, (out, (n + 4) as u32))?;
        serde_json::from_slice::<serde_json::Value>(&reply)
            .map_err(|e| format_err!("plugin replied with invalid JSON: {e}"))?;
        Ok(reply)
    }

    /// Calls a no-argument export; used to check that a runaway plugin is interrupted.
    pub fn call_raw(&mut self, name: &str) -> Result<u32> {
        self.store.set_epoch_deadline(DEADLINE_TICKS);
        let f: TypedFunc<(), u32> = self.instance.get_typed_func(&mut self.store, name)?;
        f.call(&mut self.store, ())
    }
}

/// Advances the engine's epoch every `TICK`, so a call past its deadline traps.
pub fn start_ticker(engine: &Engine) {
    let engine = engine.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(TICK);
            engine.increment_epoch();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A module whose `render` returns a pointer to a reply it claims is `claimed` bytes, or
    /// loops forever when `spin` is set after the first `call`.
    fn module_text(claimed: u32, body: &str) -> Vec<u8> {
        wat::parse_str(format!(
            r#"(module
              (memory (export "memory") 2)
              (global $calls (mut i32) (i32.const 0))
              (func (export "alloc") (param i32) (result i32) (i32.const 1024))
              (func (export "dealloc") (param i32 i32))
              (data (i32.const 2048) "{{}}")
              (func (export "render") (param i32 i32) (result i32)
                (global.set $calls (i32.add (global.get $calls) (i32.const 1)))
                {body}
                (i32.store (i32.const 2044) (i32.const {claimed}))
                (i32.const 2044)))"#
        ))
        .unwrap()
    }

    fn plugin(engine: &Engine, wasm: &[u8]) -> Plugin {
        Plugin::new(engine, &module(engine, wasm).unwrap()).unwrap()
    }

    #[test]
    fn a_reply_claiming_too_much_is_refused_before_it_is_read() {
        let engine = engine().unwrap();
        let mut ok = plugin(&engine, &module_text(2, ""));
        assert_eq!(ok.render(b"{}").unwrap(), b"{}");
        let mut big = plugin(&engine, &module_text(REPLY_LIMIT as u32 + 1, ""));
        assert!(big.render(b"{}").unwrap_err().to_string().contains("太大"));
    }

    /// A runaway call is interrupted; the module itself is still good for a new instance.
    #[test]
    fn a_runaway_call_is_stopped_and_a_new_instance_answers() {
        let engine = engine().unwrap();
        start_ticker(&engine);
        // Loops on its first call in any instance... unless told by input length not to.
        let wasm = module_text(
            2,
            "(if (i32.eq (local.get 1) (i32.const 1)) (then (loop $l (br $l))))",
        );
        let compiled = module(&engine, &wasm).unwrap();
        let mut first = Plugin::new(&engine, &compiled).unwrap();
        let at = std::time::Instant::now();
        assert!(first.render(b"x").is_err());
        assert!(at.elapsed() < Duration::from_secs(1), "{:?}", at.elapsed());
        let mut second = Plugin::new(&engine, &compiled).unwrap();
        assert_eq!(second.render(b"{}").unwrap(), b"{}");
    }
}
