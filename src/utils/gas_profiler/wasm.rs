//! A small, dependency-free WebAssembly decoder tailored to gas profiling.
//!
//! Unlike the byte-scanning heuristics elsewhere in `utils`, this walks the
//! module structurally: it decodes the type, import, function, export, code,
//! data and custom sections, then walks every function body opcode by opcode
//! (decoding immediates, so operand bytes are never mistaken for opcodes). For
//! each function it records instruction counts, loop nesting, direct callees,
//! and every call to a Soroban host import, noting whether the call sits
//! lexically inside a loop.
//!
//! It also records the constructs Soroban's VM rejects at upload time
//! (floating point, saturating float-to-int, tail calls, reference types,
//! multi-value returns and the start section), matching the wasmi feature set
//! configured in `soroban-env-host` 22.

use super::host_fns::{self, HostCategory};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MAX_FUNCTIONS: usize = 100_000;

/// A decoded import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    pub module: String,
    pub field: String,
    /// 0 = func, 1 = table, 2 = memory, 3 = global.
    pub kind: u8,
    /// Host function name for known Soroban imports.
    pub host_name: Option<String>,
    pub category: HostCategory,
}

/// A decoded export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Export {
    pub name: String,
    pub kind: u8,
    pub index: u32,
}

/// Static profile of one function body.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FunctionBody {
    /// Absolute function index (imports first).
    pub index: u32,
    /// Name from the export or `name` section, if any.
    pub name: Option<String>,
    pub body_bytes: usize,
    pub instruction_count: u64,
    pub loop_count: u32,
    pub max_loop_depth: u32,
    pub max_block_depth: u32,
    pub branch_count: u32,
    pub memory_loads: u32,
    pub memory_stores: u32,
    pub memory_grow: u32,
    pub bulk_memory_ops: u32,
    pub call_indirect_count: u32,
    /// Direct calls to other defined functions: callee index -> call sites.
    pub direct_calls: BTreeMap<u32, u32>,
    /// Host import calls: import index -> call sites.
    pub host_calls: BTreeMap<u32, u32>,
    /// Host import calls lexically inside a loop: import index -> call sites.
    pub host_calls_in_loops: BTreeMap<u32, u32>,
    /// Calls to other defined functions made from inside a loop.
    pub direct_calls_in_loops: BTreeMap<u32, u32>,
    pub float_ops: u32,
    pub rejected_features: Vec<String>,
    /// Set when the body could not be fully decoded (e.g. SIMD).
    pub decode_error: Option<String>,
}

/// A fully decoded module.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WasmModule {
    pub size_bytes: usize,
    /// `(params, results)` per type index.
    pub types: Vec<(u32, u32)>,
    pub imports: Vec<Import>,
    /// Type index for each defined function.
    pub function_types: Vec<u32>,
    pub exports: Vec<Export>,
    pub functions: Vec<FunctionBody>,
    pub global_count: u32,
    pub table_count: u32,
    pub memory_count: u32,
    pub initial_memory_pages: u64,
    pub data_segment_count: u32,
    pub data_bytes: usize,
    pub element_segment_count: u32,
    pub has_start: bool,
    /// Custom sections: name -> total bytes.
    pub custom_sections: BTreeMap<String, usize>,
}

impl WasmModule {
    /// Number of imported functions (they occupy the low function indices).
    pub fn imported_function_count(&self) -> u32 {
        self.imports.iter().filter(|i| i.kind == 0).count() as u32
    }

    /// Import record for a function index, if it refers to an import.
    pub fn import_for_function(&self, func_index: u32) -> Option<&Import> {
        self.imports
            .iter()
            .filter(|i| i.kind == 0)
            .nth(func_index as usize)
    }

    /// Defined function body for an absolute function index.
    pub fn body_for_function(&self, func_index: u32) -> Option<&FunctionBody> {
        let imported = self.imported_function_count();
        if func_index < imported {
            return None;
        }
        self.functions.get((func_index - imported) as usize)
    }

    /// Exported functions (kind 0).
    pub fn function_exports(&self) -> impl Iterator<Item = &Export> {
        self.exports.iter().filter(|e| e.kind == 0)
    }

    /// Bytes spent on custom sections that carry no runtime meaning for
    /// Soroban (everything except the contract spec/meta sections).
    pub fn strippable_custom_bytes(&self) -> usize {
        self.custom_sections
            .iter()
            .filter(|(n, _)| !is_soroban_metadata_section(n))
            .map(|(_, b)| *b)
            .sum()
    }

    pub fn total_instructions(&self) -> u64 {
        self.functions.iter().map(|f| f.instruction_count).sum()
    }
}

/// Custom sections the Soroban toolchain relies on and that must be kept.
pub fn is_soroban_metadata_section(name: &str) -> bool {
    matches!(name, "contractspecv0" | "contractenvmetav0" | "contractmetav0")
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn byte(&mut self) -> Result<u8> {
        match self.bytes.get(self.pos) {
            Some(b) => {
                self.pos += 1;
                Ok(*b)
            }
            None => bail!("unexpected end of wasm data at offset {}", self.pos),
        }
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        if self.pos + n > self.bytes.len() {
            bail!("truncated wasm data at offset {}", self.pos);
        }
        self.pos += n;
        Ok(())
    }

    fn slice(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.pos + n > self.bytes.len() {
            bail!("truncated wasm data at offset {}", self.pos);
        }
        let s = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32> {
        let mut result: u64 = 0;
        let mut shift = 0;
        loop {
            let b = self.byte()?;
            result |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 35 {
                bail!("LEB128 u32 too long at offset {}", self.pos);
            }
        }
        if result > u64::from(u32::MAX) {
            bail!("LEB128 value out of u32 range at offset {}", self.pos);
        }
        Ok(result as u32)
    }

    /// Skip a signed LEB128 of at most `max_bytes` bytes.
    fn skip_sleb(&mut self, max_bytes: usize) -> Result<()> {
        for _ in 0..max_bytes {
            if self.byte()? & 0x80 == 0 {
                return Ok(());
            }
        }
        bail!("signed LEB128 too long at offset {}", self.pos)
    }

    fn name(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        let s = self.slice(len)?;
        Ok(String::from_utf8_lossy(s).into_owned())
    }

    fn limits(&mut self) -> Result<u64> {
        let flags = self.byte()?;
        let min = self.u32()?;
        if flags & 0x01 != 0 {
            self.u32()?;
        }
        Ok(u64::from(min))
    }
}

/// True when `bytes` start with the wasm magic number.
pub fn is_wasm(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && &bytes[..4] == b"\0asm"
}

/// Decode a module. Structural errors in the section layout are returned as
/// errors; problems inside a single function body are recorded on that body
/// (`decode_error`) so one exotic function does not hide the rest.
pub fn decode(bytes: &[u8]) -> Result<WasmModule> {
    if !is_wasm(bytes) {
        bail!("not a WebAssembly module (magic bytes mismatch)");
    }
    let mut m = WasmModule {
        size_bytes: bytes.len(),
        ..Default::default()
    };
    let mut r = Reader::new(&bytes[8..]);
    let mut names: BTreeMap<u32, String> = BTreeMap::new();
    let mut code_bodies: Vec<&[u8]> = Vec::new();

    while !r.eof() {
        let id = r.byte()?;
        let size = r.u32()? as usize;
        let payload = r.slice(size)?;
        let mut s = Reader::new(payload);
        match id {
            0 => {
                let name = s.name().unwrap_or_default();
                *m.custom_sections.entry(name.clone()).or_insert(0) += size;
                if name == "name" {
                    // Function names are best effort; ignore malformed data.
                    let _ = parse_name_section(&payload[s.pos..], &mut names);
                }
            }
            1 => {
                let count = s.u32()?;
                for _ in 0..count {
                    let form = s.byte()?;
                    if form != 0x60 {
                        bail!("unsupported type form 0x{:02x}", form);
                    }
                    let params = s.u32()?;
                    s.skip(params as usize)?;
                    let results = s.u32()?;
                    s.skip(results as usize)?;
                    m.types.push((params, results));
                }
            }
            2 => {
                let count = s.u32()?;
                for _ in 0..count {
                    let module = s.name()?;
                    let field = s.name()?;
                    let kind = s.byte()?;
                    match kind {
                        0 => {
                            s.u32()?;
                        }
                        1 => {
                            s.byte()?;
                            s.limits()?;
                        }
                        2 => {
                            m.initial_memory_pages += s.limits()?;
                            m.memory_count += 1;
                        }
                        3 => {
                            s.skip(2)?;
                        }
                        other => bail!("unknown import kind {}", other),
                    }
                    let (host_name, category) = if kind == 0 {
                        (
                            host_fns::host_function_name(&module, &field).map(str::to_string),
                            host_fns::categorize(&module, &field),
                        )
                    } else {
                        (None, HostCategory::Unknown)
                    };
                    m.imports.push(Import {
                        module,
                        field,
                        kind,
                        host_name,
                        category,
                    });
                }
            }
            3 => {
                let count = s.u32()? as usize;
                if count > MAX_FUNCTIONS {
                    bail!("module declares {} functions (limit {})", count, MAX_FUNCTIONS);
                }
                for _ in 0..count {
                    m.function_types.push(s.u32()?);
                }
            }
            4 => m.table_count += s.u32()?,
            5 => {
                let count = s.u32()?;
                for _ in 0..count {
                    m.initial_memory_pages += s.limits()?;
                }
                m.memory_count += count;
            }
            6 => m.global_count = s.u32()?,
            7 => {
                let count = s.u32()?;
                for _ in 0..count {
                    let name = s.name()?;
                    let kind = s.byte()?;
                    let index = s.u32()?;
                    m.exports.push(Export { name, kind, index });
                }
            }
            8 => m.has_start = true,
            9 => m.element_segment_count = s.u32()?,
            10 => {
                let count = s.u32()? as usize;
                if count > MAX_FUNCTIONS {
                    bail!("code section has {} bodies (limit {})", count, MAX_FUNCTIONS);
                }
                for _ in 0..count {
                    let len = s.u32()? as usize;
                    code_bodies.push(s.slice(len)?);
                }
            }
            11 => {
                m.data_segment_count = s.u32()?;
                m.data_bytes = size;
            }
            _ => {}
        }
    }

    let imported = m.imported_function_count();
    for (i, body) in code_bodies.iter().enumerate() {
        let index = imported + i as u32;
        let mut f = walk_body(body, index, imported);
        f.name = names.get(&index).cloned();
        m.functions.push(f);
    }
    for e in m.exports.iter().filter(|e| e.kind == 0) {
        if e.index >= imported {
            if let Some(f) = m.functions.get_mut((e.index - imported) as usize) {
                f.name = Some(e.name.clone());
            }
        }
    }
    Ok(m)
}

fn parse_name_section(bytes: &[u8], names: &mut BTreeMap<u32, String>) -> Result<()> {
    let mut r = Reader::new(bytes);
    while !r.eof() {
        let sub_id = r.byte()?;
        let size = r.u32()? as usize;
        let payload = r.slice(size)?;
        if sub_id == 1 {
            let mut s = Reader::new(payload);
            let count = s.u32()?;
            for _ in 0..count {
                let idx = s.u32()?;
                let name = s.name()?;
                names.insert(idx, name);
            }
        }
    }
    Ok(())
}

fn is_float_opcode(op: u8) -> bool {
    matches!(op,
        0x2A | 0x2B | 0x38 | 0x39 | 0x43 | 0x44
        | 0x5B..=0x66
        | 0x8B..=0xA6
        | 0xA8..=0xAB
        | 0xAE..=0xBF)
}

enum Frame {
    Block,
    Loop,
}

fn walk_body(body: &[u8], index: u32, imported: u32) -> FunctionBody {
    let mut f = FunctionBody {
        index,
        body_bytes: body.len(),
        ..Default::default()
    };
    if let Err(e) = walk_body_inner(body, imported, &mut f) {
        f.decode_error = Some(e.to_string());
    }
    f.rejected_features.sort();
    f.rejected_features.dedup();
    f
}

fn note_rejected(f: &mut FunctionBody, what: &str) {
    if !f.rejected_features.iter().any(|x| x == what) {
        f.rejected_features.push(what.to_string());
    }
}

fn walk_body_inner(body: &[u8], imported: u32, f: &mut FunctionBody) -> Result<()> {
    let mut r = Reader::new(body);
    let local_groups = r.u32()?;
    for _ in 0..local_groups {
        r.u32()?;
        let ty = r.byte()?;
        if matches!(ty, 0x7D | 0x7C) {
            f.float_ops += 1;
            note_rejected(f, "floating-point");
        }
    }

    let mut stack: Vec<Frame> = Vec::new();
    let mut loop_depth: u32 = 0;

    while !r.eof() {
        let op = r.byte()?;
        f.instruction_count += 1;
        if is_float_opcode(op) {
            f.float_ops += 1;
            note_rejected(f, "floating-point");
        }
        match op {
            0x02..=0x04 => {
                // block type: 0x40, a single value type, or an s33 type index
                let bt = r.byte()?;
                if bt & 0x80 != 0 {
                    r.skip_sleb(4)?;
                }
                if bt & 0x80 != 0 || bt & 0x40 == 0 {
                    // non-negative s33 => type index (multi-value block)
                    note_rejected(f, "multi-value");
                }
                if op == 0x03 {
                    stack.push(Frame::Loop);
                    loop_depth += 1;
                    f.loop_count += 1;
                    f.max_loop_depth = f.max_loop_depth.max(loop_depth);
                } else {
                    stack.push(Frame::Block);
                }
                f.max_block_depth = f.max_block_depth.max(stack.len() as u32);
            }
            0x0B => {
                if let Some(Frame::Loop) = stack.pop() {
                    loop_depth = loop_depth.saturating_sub(1);
                }
            }
            0x0C | 0x0D => {
                r.u32()?;
                f.branch_count += 1;
            }
            0x0E => {
                let n = r.u32()?;
                for _ in 0..=n {
                    r.u32()?;
                }
                f.branch_count += 1;
            }
            0x10 | 0x12 => {
                if op == 0x12 {
                    note_rejected(f, "tail-call");
                }
                let callee = r.u32()?;
                if callee < imported {
                    *f.host_calls.entry(callee).or_insert(0) += 1;
                    if loop_depth > 0 {
                        *f.host_calls_in_loops.entry(callee).or_insert(0) += 1;
                    }
                } else {
                    *f.direct_calls.entry(callee).or_insert(0) += 1;
                    if loop_depth > 0 {
                        *f.direct_calls_in_loops.entry(callee).or_insert(0) += 1;
                    }
                }
            }
            0x11 | 0x13 => {
                if op == 0x13 {
                    note_rejected(f, "tail-call");
                }
                r.u32()?;
                r.u32()?;
                f.call_indirect_count += 1;
            }
            0x1C => {
                note_rejected(f, "reference-types");
                let n = r.u32()?;
                r.skip(n as usize)?;
            }
            0x20..=0x24 => {
                r.u32()?;
            }
            0x25 | 0x26 => {
                note_rejected(f, "reference-types");
                r.u32()?;
            }
            0x28..=0x35 => {
                r.u32()?;
                r.u32()?;
                f.memory_loads += 1;
            }
            0x36..=0x3E => {
                r.u32()?;
                r.u32()?;
                f.memory_stores += 1;
            }
            0x3F => {
                r.u32()?;
            }
            0x40 => {
                r.u32()?;
                f.memory_grow += 1;
            }
            0x41 => r.skip_sleb(5)?,
            0x42 => r.skip_sleb(10)?,
            0x43 => r.skip(4)?,
            0x44 => r.skip(8)?,
            0xD0 => {
                note_rejected(f, "reference-types");
                r.byte()?;
            }
            0xD1 => note_rejected(f, "reference-types"),
            0xD2 => {
                note_rejected(f, "reference-types");
                r.u32()?;
            }
            0xFC => {
                let sub = r.u32()?;
                match sub {
                    0..=7 => {
                        f.float_ops += 1;
                        note_rejected(f, "floating-point");
                        note_rejected(f, "saturating-float-to-int");
                    }
                    8 => {
                        r.u32()?;
                        r.byte()?;
                        f.bulk_memory_ops += 1;
                    }
                    9 => {
                        r.u32()?;
                        f.bulk_memory_ops += 1;
                    }
                    10 => {
                        r.byte()?;
                        r.byte()?;
                        f.bulk_memory_ops += 1;
                    }
                    11 => {
                        r.byte()?;
                        f.bulk_memory_ops += 1;
                    }
                    12 | 14 => {
                        r.u32()?;
                        r.u32()?;
                    }
                    13 => {
                        r.u32()?;
                    }
                    15..=17 => {
                        note_rejected(f, "reference-types");
                        r.u32()?;
                    }
                    other => bail!("unknown 0xFC sub-opcode {}", other),
                }
            }
            0xFD => {
                note_rejected(f, "simd");
                bail!("SIMD instructions are not supported by Soroban (decoding stopped)");
            }
            0x00 | 0x01 | 0x05 | 0x0F | 0x1A | 0x1B | 0x45..=0xC4 => {}
            other => bail!("unknown opcode 0x{:02x}", other),
        }
    }
    Ok(())
}

/// Minimal module builder used by tests, benches and fixtures. It emits only
/// what the analyzer needs (types, imports, functions, exports, code, custom
/// sections), so hand-assembled test contracts stay readable.
pub mod builder {
    fn leb(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let mut b = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                b |= 0x80;
            }
            out.push(b);
            if v == 0 {
                break;
            }
        }
    }

    fn name(s: &str, out: &mut Vec<u8>) {
        leb(s.len() as u64, out);
        out.extend_from_slice(s.as_bytes());
    }

    fn section(id: u8, payload: Vec<u8>, out: &mut Vec<u8>) {
        out.push(id);
        leb(payload.len() as u64, out);
        out.extend(payload);
    }

    /// Opcode helpers for building function bodies.
    pub mod op {
        pub fn call(idx: u32) -> Vec<u8> {
            let mut v = vec![0x10];
            super::leb(u64::from(idx), &mut v);
            v
        }
        pub fn i64_const_small(n: u8) -> Vec<u8> {
            vec![0x42, n & 0x3f]
        }
        pub fn drop() -> Vec<u8> {
            vec![0x1A]
        }
        pub fn loop_start() -> Vec<u8> {
            vec![0x03, 0x40]
        }
        pub fn block_start() -> Vec<u8> {
            vec![0x02, 0x40]
        }
        pub fn br_if(depth: u8) -> Vec<u8> {
            vec![0x0D, depth]
        }
        pub fn end() -> Vec<u8> {
            vec![0x0B]
        }
        pub fn i32_const_zero() -> Vec<u8> {
            vec![0x41, 0x00]
        }
        pub fn f64_const_zero() -> Vec<u8> {
            let mut v = vec![0x44];
            v.extend_from_slice(&[0u8; 8]);
            v
        }
    }

    /// A function to be emitted: `export` name (optional) and raw body
    /// instructions (without the trailing `end`, which is added).
    pub struct Func {
        pub export: Option<String>,
        pub code: Vec<u8>,
    }

    #[derive(Default)]
    pub struct ModuleBuilder {
        imports: Vec<(String, String)>,
        funcs: Vec<Func>,
        custom: Vec<(String, Vec<u8>)>,
        start: Option<u32>,
    }

    impl ModuleBuilder {
        pub fn new() -> Self {
            Self::default()
        }

        /// Import a host function (type `() -> i64` for simplicity). Returns
        /// its function index.
        pub fn import(mut self, module: &str, field: &str) -> Self {
            self.imports.push((module.to_string(), field.to_string()));
            self
        }

        pub fn func(mut self, export: Option<&str>, code: Vec<u8>) -> Self {
            self.funcs.push(Func {
                export: export.map(str::to_string),
                code,
            });
            self
        }

        pub fn custom(mut self, name: &str, data: Vec<u8>) -> Self {
            self.custom.push((name.to_string(), data));
            self
        }

        pub fn start(mut self, func: u32) -> Self {
            self.start = Some(func);
            self
        }

        pub fn build(self) -> Vec<u8> {
            let mut out = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
            // type 0: () -> i64 (imports); type 1: () -> () (functions)
            section(1, vec![0x02, 0x60, 0x00, 0x01, 0x7e, 0x60, 0x00, 0x00], &mut out);
            if !self.imports.is_empty() {
                let mut p = Vec::new();
                leb(self.imports.len() as u64, &mut p);
                for (m, f) in &self.imports {
                    name(m, &mut p);
                    name(f, &mut p);
                    p.push(0x00);
                    p.push(0x00);
                }
                section(2, p, &mut out);
            }
            let mut p = Vec::new();
            leb(self.funcs.len() as u64, &mut p);
            for _ in &self.funcs {
                p.push(0x01);
            }
            section(3, p, &mut out);
            let imported = self.imports.len() as u32;
            let exports: Vec<(String, u32)> = self
                .funcs
                .iter()
                .enumerate()
                .filter_map(|(i, f)| f.export.clone().map(|e| (e, imported + i as u32)))
                .collect();
            if !exports.is_empty() {
                let mut p = Vec::new();
                leb(exports.len() as u64, &mut p);
                for (n, idx) in exports {
                    name(&n, &mut p);
                    p.push(0x00);
                    leb(u64::from(idx), &mut p);
                }
                section(7, p, &mut out);
            }
            if let Some(s) = self.start {
                let mut p = Vec::new();
                leb(u64::from(s), &mut p);
                section(8, p, &mut out);
            }
            let mut p = Vec::new();
            leb(self.funcs.len() as u64, &mut p);
            for f in &self.funcs {
                let mut body = vec![0x00];
                body.extend_from_slice(&f.code);
                body.push(0x0B);
                leb(body.len() as u64, &mut p);
                p.extend(body);
            }
            section(10, p, &mut out);
            for (n, data) in self.custom {
                let mut p = Vec::new();
                name(&n, &mut p);
                p.extend(data);
                section(0, p, &mut out);
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::builder::{op, ModuleBuilder};
    use super::*;

    fn cat(ops: &[Vec<u8>]) -> Vec<u8> {
        ops.concat()
    }

    #[test]
    fn rejects_non_wasm() {
        assert!(decode(b"hello world!").is_err());
    }

    #[test]
    fn decodes_minimal_module() {
        let m = decode(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]).unwrap();
        assert!(m.functions.is_empty());
        assert_eq!(m.size_bytes, 8);
    }

    #[test]
    fn counts_host_calls_and_loops() {
        let wasm = ModuleBuilder::new()
            .import("l", "1") // 0: get_contract_data
            .import("l", "_") // 1: put_contract_data
            .func(
                Some("transfer"),
                cat(&[
                    op::call(0),
                    op::drop(),
                    op::loop_start(),
                    op::call(1),
                    op::drop(),
                    op::i32_const_zero(),
                    op::br_if(0),
                    op::end(),
                ]),
            )
            .build();
        let m = decode(&wasm).unwrap();
        assert_eq!(m.imports.len(), 2);
        assert_eq!(m.imports[0].host_name.as_deref(), Some("get_contract_data"));
        let f = &m.functions[0];
        assert_eq!(f.name.as_deref(), Some("transfer"));
        assert_eq!(f.loop_count, 1);
        assert_eq!(f.max_loop_depth, 1);
        assert_eq!(f.host_calls.get(&0), Some(&1));
        assert_eq!(f.host_calls.get(&1), Some(&1));
        assert_eq!(f.host_calls_in_loops.get(&1), Some(&1));
        assert!(f.host_calls_in_loops.get(&0).is_none());
        assert!(f.decode_error.is_none());
        // call, drop, loop, call, drop, i32.const, br_if, end, end
        assert_eq!(f.instruction_count, 9);
    }

    #[test]
    fn immediates_are_not_counted_as_opcodes() {
        // i64.const with a multi-byte LEB whose bytes look like call opcodes
        let code = vec![0x42, 0x90, 0x90, 0x10, 0x1A];
        let wasm = ModuleBuilder::new().func(Some("f"), code).build();
        let m = decode(&wasm).unwrap();
        let f = &m.functions[0];
        assert!(f.decode_error.is_none(), "{:?}", f.decode_error);
        assert_eq!(f.instruction_count, 3); // i64.const, drop, end
        assert!(f.host_calls.is_empty());
    }

    #[test]
    fn detects_soroban_rejected_features() {
        let wasm = ModuleBuilder::new()
            .func(Some("f"), cat(&[op::f64_const_zero(), op::drop()]))
            .start(0)
            .build();
        let m = decode(&wasm).unwrap();
        assert!(m.has_start);
        assert!(m.functions[0]
            .rejected_features
            .contains(&"floating-point".to_string()));
    }

    #[test]
    fn tracks_direct_calls_and_names() {
        let wasm = ModuleBuilder::new()
            .func(None, vec![])
            .func(Some("entry"), cat(&[op::call(0)]))
            .custom("contractspecv0", vec![1, 2, 3])
            .custom("producers", vec![0; 64])
            .build();
        let m = decode(&wasm).unwrap();
        assert_eq!(m.functions[1].direct_calls.get(&0), Some(&1));
        assert_eq!(m.function_exports().count(), 1);
        assert!(m.custom_sections.contains_key("contractspecv0"));
        // producers is strippable, contractspecv0 is not
        assert!(m.strippable_custom_bytes() >= 64);
        assert!(m.strippable_custom_bytes() < 64 + 20);
    }

    #[test]
    fn simd_is_reported_not_fatal() {
        let wasm = ModuleBuilder::new()
            .func(Some("f"), vec![0xFD, 0x00])
            .func(Some("g"), vec![])
            .build();
        let m = decode(&wasm).unwrap();
        assert!(m.functions[0].decode_error.is_some());
        assert!(m.functions[1].decode_error.is_none());
    }
}
