//! How far each of Mono's structures is scanned.

// The span scanned in each structure. Past these a candidate is another
// structure's business.
pub const CLASS_SPAN: usize = 0x140;
pub const FIELD_SPAN: usize = 0x28;
pub const TYPE_SPAN: usize = 0x10;
pub const IMAGE_SPAN: usize = 0x800;
pub const ASSEMBLY_SPAN: usize = 0x80;
pub const VTABLE_SPAN: usize = 0x60;
// The head words alone: the newer layout's method table starts past these.
pub const VTABLE_HEAD: usize = 0x40;
