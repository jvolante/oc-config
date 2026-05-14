---
description: Invoke when working with CUDA texture objects, `cudaResourceTypeArray`, `cudaResourceTypePitch2D`, `cudaResourceTypeLinear`, `cudaTextureDesc`, or any code that creates texture objects via `cudaCreateTextureObject`.
name: cuda_texture_reference
---

This skill contains **empirically verified** constraints and caveats derived
from probe tests. Where the CUDA Programming Guide is silent or ambiguous,
the results here are ground truth for this environment.

---

## Resource Types

### `cudaResourceTypeArray` (backed by `cudaMallocArray`)

The default backing for `CudaImage`.  Supports the full feature set:

- All filter modes: `cudaFilterModePoint`, `cudaFilterModeLinear`
- All address modes: Clamp, Border, Wrap, Mirror
- Normalized and non-normalized coordinates
- Surface writes (`cudaCreateSurfaceObject`)
- All read modes: `cudaReadModeElementType`, `cudaReadModeNormalizedFloat`
- Fetch via `tex2D<T>`, `tex1D<T>`, etc.

### `cudaResourceTypePitch2D` (backed by `cudaMallocPitch`)

2D pitched linear memory.  Useful when data is DMA-written into pitched memory
and you want to avoid an extra copy into a CUDA array.

- Fetched via `tex2D<T>`
- Supports bilinear filtering, normalized and non-normalized coordinates
- **Wrap and Mirror address modes silently produce wrong results — creation succeeds without error**
- Only Clamp and Border are safe
- No surface write support

### `cudaResourceTypeLinear` (backed by `cudaMalloc`)

1D flat linear memory. Strictly one-dimensional — there is no height or pitch.

- Fetched via `tex1Dfetch<T>` (integer coords, no filtering)
- `tex2D<T>` on a linear resource always returns `0` — not supported
- `cudaFilterModeLinear` is accepted at creation but has no effect — `tex1Dfetch` never interpolates
- Normalized coordinates: texture object creation succeeds but `tex1Dfetch` ignores them; `tex2D` returns 0
- **Wrap and Mirror address modes silently produce wrong results** — same as Pitch2D
- Only Clamp and Border are safe
- No surface write support

---

## Address Mode Compatibility — All Resource Types

**CRITICAL PITFALL:** `cudaAddressModeWrap` and `cudaAddressModeMirror` are
accepted by `cudaCreateTextureObject` without error on all three resource types,
but produce **silently incorrect results** on Pitch2D and Linear resources.
Only `cudaResourceTypeArray` handles them correctly.

| addressMode               | `cudaResourceTypeArray` | `cudaResourceTypePitch2D` | `cudaResourceTypeLinear` |
|---------------------------|-------------------------|---------------------------|--------------------------|
| `cudaAddressModeClamp`    | ✅ correct               | ✅ correct                 | ✅ correct                |
| `cudaAddressModeBorder`   | ✅ correct               | ✅ correct                 | ✅ correct                |
| `cudaAddressModeWrap`     | ✅ correct               | ⚠️ silently wrong          | ⚠️ silently wrong         |
| `cudaAddressModeMirror`   | ✅ correct               | ⚠️ silently wrong          | ⚠️ silently wrong         |

Mirror correctness was verified with a distinguishing coordinate: fetch at
index -2 on a 4-element array gives mirror→2.0, clamp→0.0, border→0.0.
Both Pitch2D and Linear returned 0.0, confirming silent clamping.

---

## Pitch2D: Verified Capability Matrix

### Filter modes

| filterMode              | normalizedCoords | Creation | Fetch correct? |
|-------------------------|------------------|----------|----------------|
| `cudaFilterModePoint`   | 0                | ✅        | ✅              |
| `cudaFilterModePoint`   | 1                | ✅        | ✅              |
| `cudaFilterModeLinear`  | 0                | ✅        | ✅              |
| `cudaFilterModeLinear`  | 1                | ✅        | ✅              |

Bilinear interpolation works with Pitch2D.  Both filter modes work correctly
with both coordinate modes on this hardware.

### Data types and read modes

Bilinear fetch at center of a 2x2 image (result = average of 4 corner values).

| Backing type | `readMode`        | `filterMode` | Creation | Fetch result                        |
|--------------|-------------------|--------------|----------|-------------------------------------|
| `float`      | `ElementType`     | Linear       | ✅        | ✅ correct float                     |
| `uint8`      | `NormalizedFloat` | Linear       | ✅        | ✅ correct `[0,1]` float             |
| `uint8`      | `ElementType`     | Linear       | ❌ fails  | —                                   |
| `uint8`      | `ElementType`     | Point        | ✅        | ✅ correct raw integer               |
| `uint16`     | `NormalizedFloat` | Linear       | ✅        | ✅ correct `[0,1]` float             |
| `uint16`     | `ElementType`     | Linear       | ❌ fails  | —                                   |
| `uint16`     | `ElementType`     | Point        | ✅        | ✅ correct raw integer               |
| `fp16`       | `ElementType`     | Linear       | ✅        | ✅ correct float (driver auto-converts) |
| `fp16`       | `NormalizedFloat` | Linear       | ❌ fails  | —                                   |

### Additional `cudaTextureDesc` fields (Pitch2D)

| Field                          | Effect                                                |
|--------------------------------|-------------------------------------------------------|
| `maxAnisotropy` (2, 8, 16)     | ✅ Creation OK, correct fetches. Anisotropic filtering works. |
| `mipmapFilterMode = Linear`    | ✅ Creation OK, correct fetches. **Silently ignored** — no mip levels. |
| `mipmapLevelBias = 1.0`        | ✅ Creation OK, correct fetches. **Silently ignored** — no mip levels. |
| `disableTrilinearOptimization` | ✅ Creation OK, correct fetches. No-op for 2D textures. |

---

## Linear (`cudaResourceTypeLinear`): Verified Capability Matrix

### Filter and fetch modes

| filterMode             | fetch fn      | normalizedCoords | Creation | Result                              |
|------------------------|---------------|------------------|----------|-------------------------------------|
| `cudaFilterModePoint`  | `tex1Dfetch`  | 0                | ✅        | ✅ correct                           |
| `cudaFilterModeLinear` | `tex1Dfetch`  | 0                | ✅        | ⚠️ creation OK, **no interpolation** |
| `cudaFilterModePoint`  | `tex2D`       | 0                | ✅        | ⚠️ always returns 0 — not supported  |
| `cudaFilterModePoint`  | `tex1Dfetch`  | 1                | ✅        | ⚠️ creation OK, normalized coords ignored by `tex1Dfetch` |

`cudaFilterModeLinear` is accepted at creation without error but `tex1Dfetch`
always uses integer coordinates — no interpolation occurs regardless of the
filter mode set in the descriptor.

### Data types and read modes (Linear)

Point filter, `tex1Dfetch`, index 1 of a 4-element array.

| Backing type | `readMode`        | Creation | Fetch result                        |
|--------------|-------------------|----------|-------------------------------------|
| `float`      | `ElementType`     | ✅        | ✅ correct float                     |
| `uint8`      | `NormalizedFloat` | ✅        | ✅ correct `[0,1]` float             |
| `uint8`      | `ElementType`     | ✅        | ✅ correct raw integer               |
| `uint16`     | `NormalizedFloat` | ✅        | ✅ correct `[0,1]` float             |
| `uint16`     | `ElementType`     | ✅        | ✅ correct raw integer               |
| `fp16`       | `ElementType`     | ✅        | ✅ correct float (driver auto-converts) |

Unlike Pitch2D, `ElementType` integer reads do **not** fail at creation for
Linear resources — the driver accepts them.  Raw integer values are accessible
via `tex1Dfetch<uint8_t>` / `tex1Dfetch<uint16_t>` regardless of filter mode.

### Additional `cudaTextureDesc` fields (Linear)

All fields tested with float, uint8+NormalizedFloat, fp16 via `tex1Dfetch`.

| Field                          | Effect                                                |
|--------------------------------|-------------------------------------------------------|
| `maxAnisotropy` (2, 16)        | ✅ Creation OK, correct fetches. Ignored for 1D. |
| `mipmapFilterMode = Linear`    | ✅ Creation OK, correct fetches. **Silently ignored**. |
| `mipmapLevelBias = 1.0`        | ✅ Creation OK, correct fetches. **Silently ignored**. |
| `disableTrilinearOptimization` | ✅ Creation OK, correct fetches. No-op for 1D. |

---

## Data type rules (all resource types)

- `ElementType + Linear filter` is rejected at creation for integer types
  (uint8, uint16) on **Pitch2D**.  On **Linear** it is accepted but filtering
  does not occur (`tex1Dfetch` is always nearest).
- `NormalizedFloat` maps unsigned integers to `[0,1]` — fetching `uint8` value
  128 returns `~0.502`, not `128.0`.  Callers must account for this.
- `fp16` backing reads back as `float` via `tex2D<float>` (Pitch2D) or
  `tex1Dfetch<float>` (Linear) — the driver auto-converts `__half → float`.
  There is no `tex2D<__half>` or `tex1Dfetch<__half>` overload in CUDA 11.
- `fp16 + NormalizedFloat` is rejected at creation on all resource types.
- Declaring integer data under a mismatched float channel descriptor
  (e.g. `cudaChannelFormatKindFloat` 32-bit over `uint8` memory) produces
  garbage — the byte is zero-padded to 32 bits, yielding denormals near zero.

### Reading integer data as unnormalized float

There is **no** texture mode that returns a raw integer value as a float with
bilinear filtering.  The two options:

1. **Point filter, ElementType, typed fetch** — `tex2D<uint8_t>` / `tex1Dfetch<uint8_t>`
   returns the raw integer.  Cast to float in the kernel.  No interpolation.

2. **Linear filter, NormalizedFloat, `tex2D<float>` / `tex1Dfetch<float>`** —
   interpolated float in `[0,1]`.  Multiply by `255.0f` or `65535.0f` in the
   kernel to recover approximate integer-scale values.

---

## `cudaTextureDesc` Field Reference

```cpp
struct cudaTextureDesc {
    cudaTextureAddressMode addressMode[3]; // per-axis: [0]=x, [1]=y, [2]=z
    cudaTextureFilterMode  filterMode;
    cudaTextureReadMode    readMode;
    int                    sRGB;           // gamma correction on read (Array only, unprobed)
    float                  borderColor[4]; // RGBA used when addressMode = Border (default {0,0,0,0})
    int                    normalizedCoords;
    unsigned int           maxAnisotropy;
    cudaTextureFilterMode  mipmapFilterMode;
    float                  mipmapLevelBias;
    float                  minMipmapLevelClamp;
    float                  maxMipmapLevelClamp;
    int                    disableTrilinearOptimization;
};
```

- **`borderColor[4]`** — hardcoded to `{0,0,0,0}` in `CudaImage::texture()`.
  Needs to be exposed for `cudaAddressModeBorder` to be configurable.
- **`addressMode[3]`** — current `TextureOptions` exposes a single `borderMode`
  for all axes.  Per-axis control is possible for non-symmetric padding.
- **`sRGB`** — not probed on Pitch2D or Linear; assume Array-only.

---

## Surface Objects

Surface objects (`cudaCreateSurfaceObject`) require `cudaResourceTypeArray`.
Not supported on Pitch2D or Linear.  For kernel-side writes to Pitch2D, use
`cudaMemcpy2DAsync` from the host or pass the raw device pointer directly.

---

## Implications for `CudaImage` Pitch2D Support

When adding `MemoryType::Pitch2D` to `CudaImage`:

1. **Enforce address mode at `texture()` time** — reject Wrap and Mirror with
   `std::runtime_error`.  The driver silently accepts them and returns wrong values.
2. **No surface object** — throw in `surface()` if called on a Pitch2D image.
3. **`ElementType + Linear` for integer types** — reject at `texture()` time
   with a message directing callers to use `NormalizedFloat`.
4. **Expose `borderColor`** — currently hardcoded to `{0,0,0,0}`.
5. **Mipmap fields** — safe to expose but no-ops on non-mipmapped resources.
6. **Copy paths** — `writeFromGPUBuffer` / `writeFromCPUBuffer` use
   `cudaMemcpy2DToArrayAsync` (Array-only).  Pitch2D copies must use
   `cudaMemcpy2DAsync` with the pitched pointer.

---

## Quick Decision Guide

```
Need kernel-side writes?             → CudaArray only
Need bilinear interpolation?
  float or fp16 data                 → Array or Pitch2D
  integer data, result in [0,1]      → Array or Pitch2D + NormalizedFloat
  integer data, raw value            → Point filter only (any resource type)
  1D linear data, any type           → Linear + tex1Dfetch (no interpolation)
Need Wrap or Mirror address mode?    → CudaArray only (silently wrong elsewhere)
Need 2D fetch from flat cudaMalloc?  → Not possible — use Pitch2D instead
Data in pitched device mem already?  → Pitch2D (avoids copy to CudaArray)
Need mipmap levels?                  → CudaArray with cudaMipmappedArray
```
