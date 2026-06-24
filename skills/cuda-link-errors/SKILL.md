---
description: Diagnose and fix CUDA/nvcc link errors and CCCL header bugs. Invoke when a build fails with "multiple definition" of a `cuda::` / `cuda::std::` symbol, undefined references to CUDA kernel wrappers in a shared library, warning `#20054-D` about dynamic initialization of `__shared__` variables, or other toolkit-version-specific link/compile issues from `<cuda/pipeline>`, `<cuda/barrier>`, or libcu++ headers.
name: cuda-link-errors
---

# CUDA / CCCL link-error troubleshooting

Decision trees for non-obvious link and compile failures caused by the CUDA
toolkit and CCCL (libcu++) headers, rather than by user code. These are
**toolkit-version-specific**: a clean build on one CUDA version may fail on
another. Always identify the CUDA version in the *failing* environment before
choosing a fix.

---

## Symptom: `multiple definition of cuda::...` at host link time

Example:

```
ld: .../offset_only_correction.cu.o: in function
  `cuda::__3::pipeline<(cuda::__3::thread_scope)2>::__barrier_try_wait_parity_impl(...)':
.../cuda11.4-cuda_cccl-11.4.298/include/cuda/pipeline:242: multiple definition of
  `cuda::__3::pipeline<...>::__barrier_try_wait_parity(...)';
  .../gain_only_correction.cu.o: ... first defined here
```

### Root cause

This is a **CCCL bug**, not your code. The function comes from a system header
(here `<cuda/pipeline>`), and the header gave it **external (non-`inline`)
linkage**, so every translation unit that includes it emits a strong definition.
When two such `.cu` objects are linked into one shared library, the host linker
sees duplicate strong symbols.

In CUDA 11.4 specifically, `<cuda/pipeline>` pulls in
`cuda/std/detail/__config`, which defines `_LIBCUDACXX_INLINE_VISIBILITY` as just
`__host__ __device__` (no `inline`) — conflicting with the proper definition in
`cuda/std/detail/libcxx/include/__config` (`_LIBCUDACXX_HIDE_FROM_ABI`, which is
`inline` + internal linkage). The wrong one wins for this header.

### How to confirm

1. Note the CUDA version from the header path in the error
   (`.../cudaXX.Y-cuda_cccl-.../include/...`).
2. Check whether the symbol still exists in a newer toolkit:
   `grep -n '<symbol-base-name>' <newer-cccl>/include/cuda/<header>`.
   For the pipeline barrier helper above, it was **removed entirely in CUDA 12.x**.
3. Demangle symbols when reading the error: pipe through `c++filt` (from
   `binutils`) — e.g. `c++filt <<< '_ZN4cuda...'`.

### Fix ranking (prefer earliest that is viable)

1. **Upgrade the CUDA/CCCL toolkit** if the symbol is gone in a newer version and
   the upgrade is feasible. Caveat: on embedded targets (Jetson/Orin) the CUDA
   version is tied to JetPack and to the deployment runtime — bumping it touches
   nix pins, ABI, and device images, so it is often a large, separate effort, not
   a quick PR fix.
2. **Tolerate the duplicate at link time** when the two definitions are *identical*
   instantiations of the same inline helper from the same header (they always are
   for this class of bug). Add to the offending target:
   ```cmake
   target_link_options(<target> PRIVATE "LINKER:--allow-multiple-definition")
   ```
   Minimal, correctly scoped, and safe because the definitions are byte-identical.
3. **Isolate the header to a single translation unit** so only one `.cu` emits the
   symbol (move the pipeline/barrier usage behind one `.cu` + thin wrappers).
   Larger refactor; use only if (1) and (2) are unavailable.

### TRAP: do NOT use `-fvisibility=hidden` for this

Hidden visibility controls the **dynamic** symbol table (what a `.so` exports),
not **static** link-time de-duplication. It will **not** silence a
"multiple definition" error within a single link. Worse, applied to your `.cu`
files it also hides *your own* public kernel entry points (e.g. `update_offsets`,
`apply_offsets`) from the `.so`, which then produces a *second* failure:

```
ld: undefined reference to `ica::enhance::update_offsets(...)'
```
in any dependent executable (tests, benchmarks) that links against the library.
If you see kernel-wrapper "undefined reference" errors after touching visibility,
this is the cause — revert the visibility change.

---

## Symptom: warning `#20054-D` dynamic initialization of `__shared__`

```
warning #20054-D: dynamic initialization is not supported for a function-scope
static __shared__ variable within a __device__/__global__ function
```

Typically from the canonical CCCL pipeline pattern:

```cpp
__shared__ ::cuda::pipeline_shared_state<::cuda::thread_scope::thread_scope_block, 2> shared;
```

### Verdict: benign — ignore

This is the NVIDIA-documented usage for `pipeline_shared_state`; the constructor
is effectively a no-op the runtime handles. It is only a warning, does not affect
correctness, and is gone in CUDA 12.x. Do not contort the code to silence it.
If suppression is truly required, use `-diag-suppress 20054` rather than changing
the pattern.

---

## General guidance

- **Toolkit parity matters.** A clean *local* build proves nothing about a
  toolkit-version-specific link bug if local CUDA differs from CI CUDA. Validate
  the actual fix in the failing environment (CI, or an arch/version-matched dev
  shell), not just locally.
- **Whole-program vs separable compilation.** These host-symbol duplications are a
  *host* link issue; toggling `CMAKE_CUDA_SEPARABLE_COMPILATION` affects *device*
  symbol resolution and generally does not fix them — don't reach for it first.
- **Read mangled names with `c++filt`** (binutils) to identify whether a symbol is
  user code or a CCCL/libcu++ internal before deciding on a fix.
