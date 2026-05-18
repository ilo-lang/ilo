# ilo Memory Model and Cycle Freedom

Reference notes on ilo's runtime memory model, written up while
investigating the "improve cycle detection" memory brief. Captures
*why* ilo does not have (and does not currently need) a cycle
collector, what would change that, and where the foundation already
exists.

## TL;DR

ilo's runtime is reference-counted (Arc in the tree interpreter,
custom RC on `HeapObj` in the VM). Pure RC cannot reclaim reference
cycles. Most RC languages pair RC with a cycle collector. **ilo does
not, because the surface language is structurally cycle-free.** Every
heap-allocated value in both the tree interpreter and the VM is
immutable after construction, and there is no surface-language
construct that lets a user install a reference back into a
previously-allocated holder.

The static foundation for a future collector is already in place:
`Type::can_form_cycle` in `src/ast/mod.rs` classifies each static type
as cycle-capable or cycle-incapable, and is tested against the cases
that would matter the day ilo grows a mutation primitive. Until then
the classifier sits unused at runtime and serves as a regression
surface for the invariant.

## Tree interpreter (`src/interpreter/mod.rs`)

```rust
pub enum Value {
    Number(f64),
    Text(Arc<String>),
    Bool(bool),
    Nil,
    List(Arc<Vec<Value>>),
    Map(Arc<HashMap<MapKey, Value>>),
    Record { type_name: String, fields: HashMap<String, Value> },
    Ok(Box<Value>),
    Err(Box<Value>),
    FnRef(String),
    Closure { fn_name: String, captures: Vec<Value> },
}
```

Every shared structure is wrapped in `Arc` of an *immutable* payload.
Mutation builtins (`mset`, `slc`, list updates) consume an owned `Arc`
and use `Arc::make_mut`, which clones if the strong count is greater
than one. The value bound to the new key is an already-evaluated
`Value`; it is impossible for that value to be a reference forward to
the map being mutated. Closures capture by value. Records use plain
`HashMap` per record (no sharing) and are reconstructed by `with`.

## VM (`src/vm/mod.rs`)

```rust
enum HeapObj {
    Str(String),
    List(Vec<NanVal>),
    ListView { src: NanVal, start: usize, len: usize },
    Map(HashMap<MapKey, NanVal>),
    Record { type_info: Rc<TypeInfo>, fields: Box<[NanVal]> },
    OkVal(NanVal),
    ErrVal(NanVal),
    Closure { kind: FnRefKind, id: u32, captures: Vec<NanVal> },
}
```

The VM has a manual RC on `HeapObj` (the existing SAFETY comments
around `OP_RECSETFIELD` document the invariants). The closest thing
to in-place mutation is `OP_RECSETFIELD`, which the compiler emits
only against records the same instruction sequence just allocated via
`OP_RECNEW_EMPTY` or `OP_RECCOPY`. At that moment the record has
refcount 1 and is not reachable from any other value; the field being
stored is itself a NanVal computed before the assignment. There is no
way to weave the record's own NanVal back into one of its own fields
from the source language.

## Closure captures

Both engines snapshot captures at the `make_closure` site. There is no
by-reference capture form. A closure cannot mutate a captured value in
a way that would point another value back at the closure.

## Why cycles are unreachable

The combination of immutable post-construction heap objects,
copy-on-share for the "mutable" container builtins, by-value closure
captures, and the absence of any field-assignment expression means
the static structure of an ilo program cannot produce a cycle in the
runtime heap. Refcounts will always reach zero through normal drop
chains.

## What flips this

If ilo ever grows one of these features, cycles become reachable and
a real cycle collector becomes necessary:

1. A field-assignment expression on records (`r.x = y`).
2. A mutable reference cell type (`Ref t`, similar to OCaml `ref` or
   Rust `RefCell`).
3. By-reference closure captures.
4. Any FFI builtin that exposes a writable handle into a previously
   constructed heap object.

When that day comes, `Type::can_form_cycle` is the pruning oracle a
Bacon-Rajan-style synchronous cycle collector would consult to decide
whether an allocation needs a colour field, an incoming entry in the
roots set, or trial-deletion treatment at all.

## The classifier

```rust
impl Type {
    pub fn can_form_cycle<F>(&self, resolve_record: &F) -> bool
    where F: Fn(&str) -> Option<Vec<Type>>;
}
```

Returns `true` if a runtime value of the given static type could
possibly participate in a cycle. Defaults to `true` whenever
information is missing (unknown `Named`, `Any`, function types). It
is always sound to mark a type cycle-capable; the cost is unnecessary
scanning. The unsound case is the reverse: marking a cycle-capable
type clean would let a real cycle leak forever.

Tests in `src/ast/mod.rs` cover: primitives, list/map/optional/result
of primitives, `Any`, `Fn`, unknown `Named`, primitive records,
records with primitive collections, records with `Any` or `Fn` fields,
self-referential records, mutually recursive records, and lists of
records.

## Status of the original brief

The memory-2 brief (`zero-gap-specs/briefs/memory/2-cycle-detection-brief.md`)
asks for two optimisations to "ilo's runtime cycle detector":
incremental detection, and type-based pruning. Both presume a
detector that does not exist. The right follow-up is to retire or
rewrite the brief as "when ilo grows mutation, here is what a cycle
collector should look like and what we have already pre-built".
