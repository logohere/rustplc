//! Located configuration-global matching from OpenPLC `located_globals.c`.

/// Result of joining located-variable storage identities to configuration globals.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LocatedGlobalJoin {
    /// Located-variable indices whose storage identity is a configuration global.
    pub indices: Vec<usize>,
    /// Number of non-null configuration globals found in the located-variable list.
    pub matched: usize,
}

/// Join by storage identity, preserving located-variable order and aliases.
/// `None` represents the null pointer used by the C ABI for an unbound entry.
pub fn join_located_globals(
    located: &[Option<usize>],
    globals: &[Option<usize>],
) -> LocatedGlobalJoin {
    if located.is_empty() || globals.is_empty() {
        return LocatedGlobalJoin::default();
    }
    let mut indices = Vec::new();
    for (index, pointer) in located.iter().enumerate() {
        let Some(pointer) = pointer else { continue };
        if globals.iter().flatten().any(|global| global == pointer) {
            indices.push(index);
        }
    }
    let matched = globals
        .iter()
        .flatten()
        .filter(|global| located.iter().flatten().any(|pointer| pointer == *global))
        .count();
    LocatedGlobalJoin { indices, matched }
}

#[cfg(test)]
mod tests {
    use super::{join_located_globals, LocatedGlobalJoin};

    #[test]
    fn matches_identity_and_counts_unmatched_globals() {
        let located = [Some(10), None, Some(20), Some(10)];
        let globals = [Some(20), Some(30), Some(10), None];
        assert_eq!(
            join_located_globals(&located, &globals),
            LocatedGlobalJoin {
                indices: vec![0, 2, 3],
                matched: 2,
            }
        );
    }
}
