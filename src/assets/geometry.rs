//! Split out of the parent module so that edits there do not move these
//! definitions: types inside `GameState` are inlined into many codegen
//! units, and moving one recompiles all of them (#266).

/// How a replacement's geometry is constrained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Geometry {
    /// Any size: the caller stretches the image over its frame.
    Free,
    /// The shadowed copy of the same stem decides. Its dimensions are decoded
    /// and a replacement that differs is refused; a headerless `.pix` decodes
    /// at them.
    Shipped,
}
