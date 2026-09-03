# 09. Reverse Engineering

When: Analyzing proprietary binaries, firmware blobs, obfuscated bytecodes, or complex verification routines without source code.

## Mental model
Reverse engineering is reconstructing high-level intent and logical invariants from machine representations. Never attempt to read every instruction — isolate the specific data transformation, validation checkpoint, or decryption routine, and use automated constraint solvers (Z3) and symbolic execution (angr) to solve for the key rather than manually reversing complex arithmetic.

## Attack arc
- Triage & Classification:
  - File format (`file`, `readelf`, `rabin2`): Architecture (x86/ARM/MIPS/RISC-V/WASM), packing (`upx -d`), stripped status, compilation language (C, C++, Go, Rust, C#, Java).
  - Extract strings, symbol tables, imports, and cryptographic constants (AES s-boxes, MD5/SHA initial vectors via `findcrypt`).
- Static & Decompiler Analysis:
  - Open in decompiler (Ghidra, IDA Pro, Binary Ninja, Cutter).
  - Trace Cross-References (XREFs) from key strings (e.g. "Wrong", "Correct", "Key", "License", format strings).
  - Reconstruct custom structures, arrays, and function prototypes.
- Automated Constraint Solving with Z3:
  - For validation routines consisting of complex arithmetic, bitwise XOR/AND/OR, matrix multiplication, or hashing:
  - Translate assembly/C logic into Python Z3 SMT Solver (`BitVec`, `Solver`, `add()`, `check()`, `model()`) and solve for the exact flag bytes in seconds.
- Symbolic Execution with angr:
  - For complex binaries with thousands of branch paths:
  - Configure `angr.Project()`, create simulation manager, specify `find=[<win_addr>]`, `avoid=[<fail_addr>]`, and evaluate concrete input `state.solver.eval(flag_input)`.
- Dynamic Analysis & Emulation:
  - Hook functions and monitor registers with GDB + GEF, `ltrace`, `strace`.
  - Emulate partial routines or non-native architectures using `Qiling` framework or `Unicorn` engine.
- Deobfuscation & Virtual Machines (VM Crackmes):
  - *Control Flow Flattening (OLLVM):* Identify the state variable dispatcher and patch jumps to reconstruct linear basic blocks.
  - *Custom Bytecode VMs:* Identify the fetch-decode-execute loop (instruction pointer, opcode dispatch table, virtual registers/stack). Dump the bytecode and write a custom disassembler.
- Language-Specific RE:
  - *Go:* Reconstruct stripped function names using `pclntab` / `gopclntab` structures (`GoReSym`).
  - *Rust:* Demangle symbols (`rustfilt`), trace `Result`/`Option` and slice bounds.
  - *WebAssembly:* Decompile via `wasm-decompile`, `wabt`, or convert to C.

## Key techniques & primitives
- Z3 Constraint Template:
  `s = Solver(); flag = [BitVec(f'b_{i}', 8) for i in range(N)]; ... s.check(); m = s.model(); print(bytes([m[b].as_long() for b in flag]))`.
- Anti-Debug Patching: Bypass `ptrace(PTRACE_TRACEME)` or `IsDebuggerPresent` by NOPing the check or modifying the return value in GDB.

## Tells & signals
- Giant switch statements in a loop = Custom Bytecode VM dispatcher or OLLVM flattened state machine.
- Magic constants (`0x67452301`, `0x9e3779b9` TEA delta, `0x63` AES sbox) = standard cryptographic algorithms.
- Stripped Go binary = use symbol recovery scripts before attempting manual analysis.
