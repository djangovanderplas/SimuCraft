# ADR 0002: Shapes of the Phase 0 contracts

- Status: accepted
- Date: 2026-11-07
- Work package: WP0.3
- Code: `crates/simucraft-contracts`

These choices are frozen at gate G0. A change to them is a contract change:
it gets its own pull request (see CLAUDE.md).

1. **All values are `f64` slots.** Signals, states and parameters are stored
   as `f64`. Integer and boolean types keep their meaning in the block
   (casting, wrapping, saturation). One storage type keeps the runtime
   simple. Integer values are exact up to 2^53. Fixed-point types are a
   later item and will need their own decision.
2. **Each port is one contiguous slot range.** The compiler flattens buses
   and muxes and adds copy blocks where a port gathers from more than one
   source. The output ports of one block are adjacent.
3. **A physical network is one block.** The acausal compiler emits a
   `sys.Dae` block that has continuous states, algebraic variables and a
   `residuals` method. The runtime does not need a second kind of model.
4. **Blocks get a `Ctx` with their own slices only.** The runtime gathers
   inputs into a scratch buffer before each call. This costs one copy per
   input, but needs no `unsafe` code and no aliasing rules (see ADR 0001 for
   the measured cost).
5. **The compiled model is plain JSON-serializable data.** You can write it
   by hand, so the runtime (WP1.2) can start before the compiler (WP1.1)
   exists. `tests/fixtures/mass_spring_damper.json` is the first example.
6. **The Model API is JSON-RPC 2.0.** The method goes in `method` and the
   arguments in `params`. Edit commands take `dry_run` and return an
   `EditResult` with the semantic diff. `METHODS` lists every method, and a
   fixture for each method is in `tests/fixtures/requests.json`.
7. **A diagnostic can carry a fix as API commands.** `Fix.commands` holds
   Model API edits that apply the fix, so the editor and agents can apply a
   fix with one call.
8. **Diagnostic codes have a prefix for each area:** `F` files, `C`
   compiler, `R` runtime, `A` API. The format crate lists its codes in
   `simucraft_format::codes`.

## Amendment 1 (2026-11-08, gate G1)

Two gaps in the block contract came up in WP1.1 and WP1.3. Both made the
compiler know about one block type by name.

1. **A block spec can declare a variable number of inputs.**
   `BlockSpec::variadic_inputs` names a port prefix and the parameter that sets
   the count (a whole number, or the length of a text or a vector).
   `BlockSpec::input_names` gives the port names, so the compiler, the service
   and the editor agree. `core.Sum` uses it: one input `u1`, `u2`, and so on
   for each sign. Before, the compiler had a special case for `core.Sum`.
2. **`BlockType::build` gets the resolved sample time.** It is the block's
   `ts` field, or else the rate that the block inherits from its sources. If
   the compiler does not know the rate yet, it passes `Inherited` and builds
   the block again later. Before, the compiler passed the `ts` field as a
   parameter named `ts`, and an inherited block could not know its rate.
   `core.PID` now takes its rate from this argument and has no `ts`
   parameter.

The new field has a serde default, so the JSON fixtures need no change.
