//! Compatibility shim during the next→main sync (ILO-422).
//!
//! `next` extracted runtime primitives (Value, MapKey, RuntimeError,
//! tree-bridge dispatch) into `src/runtime/`. `main` still keeps them in
//! `src/interpreter/`. To keep both branches buildable at the merge point,
//! `runtime` re-exports the live interpreter module. Hard split deferred.
pub use crate::interpreter::*;

pub mod json {
    #[allow(unused_imports)]
    pub use crate::interpreter::json::*;
}
