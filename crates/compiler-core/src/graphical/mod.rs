mod connection_types;
mod context;
mod narrow;
mod path_tree;
mod walker;

#[cfg(test)]
mod regression_tests;

pub use connection_types::{BlockInfos, BlockIo, TypeContext};
pub use context::build_type_context;
pub use walker::{
    emit_fbd_value, emit_fbd_value_with_context, emit_ld_body, emit_ld_body_with_context,
    emit_ld_value, emit_ld_value_with_context, EmitResult, SyntheticVar,
};
