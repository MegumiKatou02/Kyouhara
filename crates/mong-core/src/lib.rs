//! mong-core — máy ảo cốt truyện của Mộng Engine.
//!
//! Crate này KHÔNG biết gì về nền tảng: không render, không audio, không I/O.
//! Nó chỉ thực thi IR và phát ra [`vm::VmEvent`] để tầng runtime trình diễn.
//! Bất biến quan trọng: thực thi hoàn toàn xác định (deterministic) — cùng
//! cốt truyện + cùng chuỗi input luôn cho cùng chuỗi event, trên mọi nền tảng.

pub mod ir;
pub mod story;
pub mod vars;
pub mod vm;

pub use ir::{ChoiceArm, Cond, CondOp, Effect, Instr, SayOpts, SetOp, StagePos, Value};
pub use story::{Node, Story};
pub use vars::VarStore;
pub use vm::{
    LoadOutcome, PresentedChoice, SaveSlot, Snapshot, Vm, VmError, VmEvent, VmStatus, SAVE_VERSION,
};
