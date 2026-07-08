//! Máy ảo cốt truyện: máy trạng thái tường minh + snapshot/rollback.
//!
//! Vòng đời: `Running` thực thi IR đến khi gặp lệnh cần chờ
//! (`Say`/`Wait` → `AwaitAdvance`, `Choice` → `AwaitChoice`) hoặc `Ended`.
//! Mỗi lần dừng chờ, VM tự chụp snapshot — nền tảng của rollback,
//! time-travel và save/load.

use crate::ir::{ChoiceArm, Instr, SayOpts, StagePos, StringKey};
use crate::story::Story;
use crate::vars::VarStore;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Sự kiện trình diễn — hợp đồng giữa core và tầng runtime/renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum VmEvent {
    Say {
        speaker: Option<String>,
        text: StringKey,
        opts: SayOpts,
    },
    Show {
        character: String,
        pose: Option<String>,
        pos: StagePos,
    },
    Hide {
        character: String,
    },
    SceneChanged {
        scene: String,
        transition: Option<String>,
    },
    Choices {
        arms: Vec<PresentedChoice>,
    },
    Wait {
        ms: u32,
    },
    Sfx {
        asset: String,
    },
    Bgm {
        asset: Option<String>,
    },
    Ext {
        command: String,
        args: serde_json::Value,
    },
    NodeEntered {
        node: String,
    },
    Ended,
}

/// Một lựa chọn đã lọc điều kiện, kèm chỉ số để gọi [`Vm::choose`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentedChoice {
    pub index: usize,
    pub text: StringKey,
}

/// Trạng thái vòng đời của VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmStatus {
    Idle,
    Running,
    AwaitAdvance,
    AwaitChoice,
    Ended,
}

/// Lỗi runtime của VM.
#[derive(Debug, Clone, PartialEq)]
pub enum VmError {
    UnknownNode(String),
    TypeMismatch { var: String },
    NotAwaitingAdvance,
    NotAwaitingChoice,
    InvalidChoice(usize),
    CallStackUnderflow,
    NotStarted,
    BadSaveVersion(u32),
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmError::UnknownNode(n) => write!(f, "khong tim thay node '{n}'"),
            VmError::TypeMismatch { var } => write!(f, "sai kieu tren bien '{var}'"),
            VmError::NotAwaitingAdvance => write!(f, "vm khong o trang thai cho advance"),
            VmError::NotAwaitingChoice => write!(f, "vm khong o trang thai cho lua chon"),
            VmError::InvalidChoice(i) => write!(f, "chi so lua chon {i} khong hop le"),
            VmError::CallStackUnderflow => write!(f, "return khi call stack rong"),
            VmError::NotStarted => write!(f, "vm chua start()"),
            VmError::BadSaveVersion(v) => {
                write!(f, "save version {v} khong ho tro (ho tro: {SAVE_VERSION})")
            }
        }
    }
}
impl std::error::Error for VmError {}

/// Con trỏ chương trình: node + đường vào các nhánh `if` lồng nhau + chỉ số lệnh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Cursor {
    node: usize,
    /// (chỉ số lệnh `if` ở block cha, nhánh đã vào: true=then, false=else)
    parents: Vec<(usize, bool)>,
    ip: usize,
}

/// Phiên bản định dạng save slot — tăng khi cấu trúc [`SaveSlot`] đổi
/// (kèm migration ở tầng đọc).
pub const SAVE_VERSION: u32 = 1;

/// Save slot = snapshot + metadata + thông tin nhận diện cốt truyện.
/// Core chỉ sinh/nạp dữ liệu; chỗ ghi (file/localStorage) là việc của shell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveSlot {
    pub save_version: u32,
    pub story_format_version: u32,
    pub story_hash: u64,
    /// Node id tại điểm lưu — cho UI hiển thị và cho fallback khi cốt truyện đã đổi.
    pub node: String,
    /// Nhãn do shell đặt (tên slot, chương...).
    pub label: String,
    /// Thời điểm lưu, do shell cung cấp — core không bao giờ đọc đồng hồ.
    #[serde(default)]
    pub created_at: Option<String>,
    snapshot: Snapshot,
}

/// Kết quả nạp save.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadOutcome {
    /// Cốt truyện y nguyên — khôi phục đúng điểm lưu, kèm event phát lại.
    Exact(Vec<VmEvent>),
    /// Cốt truyện đã đổi — biến được giữ, chạy lại từ đầu node cùng id.
    /// Runtime PHẢI hiển thị cảnh báo cho người chơi.
    NodeFallback { node: String, events: Vec<VmEvent> },
}

impl Vm {
    /// Tạo save slot từ điểm chờ gần nhất. `None` nếu VM chưa có điểm chờ nào
    /// (chưa `start()`).
    pub fn save(&self, label: impl Into<String>, created_at: Option<String>) -> Option<SaveSlot> {
        let snapshot = self.snapshots.last().cloned()?;
        let node = self.story.nodes[snapshot.cursor.node].id.clone();
        Some(SaveSlot {
            save_version: SAVE_VERSION,
            story_format_version: self.story.format_version,
            story_hash: self.story.hash64(),
            node,
            label: label.into(),
            created_at,
            snapshot,
        })
    }

    /// Nạp một save slot.
    /// - Hash khớp: khôi phục đúng điểm lưu ([`LoadOutcome::Exact`]).
    /// - Hash lệch: giữ biến, chạy lại từ đầu node cùng id
    ///   ([`LoadOutcome::NodeFallback`]) — quy tắc tương thích của tài liệu
    ///   thiết kế: "thử khớp theo node id và cảnh báo thay vì crash".
    ///   Node id không còn → [`VmError::UnknownNode`].
    ///
    /// Lịch sử rollback bị xoá khi load (không time-travel xuyên hai timeline).
    pub fn load(&mut self, slot: &SaveSlot) -> Result<LoadOutcome, VmError> {
        if slot.save_version != SAVE_VERSION {
            return Err(VmError::BadSaveVersion(slot.save_version));
        }
        self.snapshots.clear();
        if slot.story_hash == self.story.hash64() {
            let replay = self.restore(&slot.snapshot);
            return Ok(LoadOutcome::Exact(replay));
        }
        // Cốt truyện đã đổi: vị trí giữa node (ip/parents/calls) không còn
        // tin được — về đầu node cùng id, chỉ mang theo kho biến.
        let idx = self
            .story
            .node_index(&slot.node)
            .ok_or_else(|| VmError::UnknownNode(slot.node.clone()))?;
        self.vars = slot.snapshot.vars.clone();
        self.calls.clear();
        self.pending.clear();
        self.cursor = Cursor {
            node: idx,
            parents: Vec::new(),
            ip: 0,
        };
        self.status = VmStatus::Running;
        let mut events = vec![VmEvent::NodeEntered {
            node: slot.node.clone(),
        }];
        events.extend(self.run()?);
        Ok(LoadOutcome::NodeFallback {
            node: slot.node.clone(),
            events,
        })
    }
}

/// Ảnh chụp toàn bộ trạng thái VM tại một điểm chờ — đơn vị của rollback và save.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    cursor: Cursor,
    calls: Vec<Cursor>,
    vars: VarStore,
    pending: Vec<ChoiceArm>,
    status: VmStatus,
    /// Sự kiện đã phát tại điểm chờ này — phát lại khi restore để renderer vẽ đúng.
    replay: Vec<VmEvent>,
}

/// Máy ảo cốt truyện.
pub struct Vm {
    story: Story,
    cursor: Cursor,
    calls: Vec<Cursor>,
    vars: VarStore,
    pending: Vec<ChoiceArm>,
    status: VmStatus,
    snapshots: Vec<Snapshot>,
    snapshot_cap: usize,
}

impl Vm {
    pub fn new(story: Story) -> Result<Self, VmError> {
        if story.node_index(&story.start).is_none() {
            return Err(VmError::UnknownNode(story.start.clone()));
        }
        Ok(Vm {
            story,
            cursor: Cursor {
                node: 0,
                parents: Vec::new(),
                ip: 0,
            },
            calls: Vec::new(),
            vars: VarStore::default(),
            pending: Vec::new(),
            status: VmStatus::Idle,
            snapshots: Vec::new(),
            snapshot_cap: 400,
        })
    }

    pub fn status(&self) -> VmStatus {
        self.status
    }

    pub fn vars(&self) -> &VarStore {
        &self.vars
    }

    /// Bắt đầu (hoặc chơi lại từ đầu).
    pub fn start(&mut self) -> Result<Vec<VmEvent>, VmError> {
        let start_idx = self
            .story
            .node_index(&self.story.start)
            .ok_or_else(|| VmError::UnknownNode(self.story.start.clone()))?;
        self.vars = VarStore::from(self.story.variables.clone());
        self.cursor = Cursor {
            node: start_idx,
            parents: Vec::new(),
            ip: 0,
        };
        self.calls.clear();
        self.pending.clear();
        self.snapshots.clear();
        self.status = VmStatus::Running;
        let mut ev = vec![VmEvent::NodeEntered {
            node: self.story.start.clone(),
        }];
        ev.extend(self.run()?);
        Ok(ev)
    }

    /// Người chơi bấm tiếp (sau `Say`) hoặc runtime hết giờ (sau `Wait`).
    pub fn advance(&mut self) -> Result<Vec<VmEvent>, VmError> {
        if self.status != VmStatus::AwaitAdvance {
            return Err(VmError::NotAwaitingAdvance);
        }
        self.status = VmStatus::Running;
        self.run()
    }

    /// Người chơi chọn lựa chọn thứ `index` trong danh sách đã trình bày.
    pub fn choose(&mut self, index: usize) -> Result<Vec<VmEvent>, VmError> {
        if self.status != VmStatus::AwaitChoice {
            return Err(VmError::NotAwaitingChoice);
        }
        let arm = self
            .pending
            .get(index)
            .cloned()
            .ok_or(VmError::InvalidChoice(index))?;
        for e in &arm.effects {
            self.vars.apply(e)?;
        }
        self.pending.clear();
        match arm.target {
            Some(t) => {
                let idx = self
                    .story
                    .node_index(&t)
                    .ok_or(VmError::UnknownNode(t.clone()))?;
                self.cursor = Cursor {
                    node: idx,
                    parents: Vec::new(),
                    ip: 0,
                };
                self.status = VmStatus::Running;
                let mut ev = vec![VmEvent::NodeEntered { node: t }];
                ev.extend(self.run()?);
                Ok(ev)
            }
            None => {
                self.status = VmStatus::Ended;
                let ev = vec![VmEvent::Ended];
                self.push_snapshot(&ev);
                Ok(ev)
            }
        }
    }

    /// Quay lại điểm chờ ngay trước đó. Trả về sự kiện cần phát lại,
    /// hoặc `None` nếu không còn gì để lùi.
    pub fn rollback(&mut self) -> Option<Vec<VmEvent>> {
        if self.snapshots.len() >= 2 {
            self.snapshots.pop();
        }
        let snap = self.snapshots.last().cloned()?;
        self.apply_snapshot(&snap);
        Some(snap.replay)
    }

    /// Ảnh chụp hiện tại (dành cho save slot).
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.snapshots.last().cloned()
    }

    /// Khôi phục từ một ảnh chụp (load save). Trả về sự kiện cần phát lại.
    pub fn restore(&mut self, snap: &Snapshot) -> Vec<VmEvent> {
        self.apply_snapshot(snap);
        self.snapshots.push(snap.clone());
        snap.replay.clone()
    }

    fn apply_snapshot(&mut self, s: &Snapshot) {
        self.cursor = s.cursor.clone();
        self.calls = s.calls.clone();
        self.vars = s.vars.clone();
        self.pending = s.pending.clone();
        self.status = s.status;
    }

    fn push_snapshot(&mut self, replay: &[VmEvent]) {
        self.snapshots.push(Snapshot {
            cursor: self.cursor.clone(),
            calls: self.calls.clone(),
            vars: self.vars.clone(),
            pending: self.pending.clone(),
            status: self.status,
            replay: replay.to_vec(),
        });
        if self.snapshots.len() > self.snapshot_cap {
            self.snapshots.remove(0);
        }
    }

    /// Lấy block lệnh hiện hành theo con trỏ (đi xuyên các nhánh `if`).
    fn current_block(&self) -> &[Instr] {
        let mut blk: &[Instr] = &self.story.nodes[self.cursor.node].body;
        for (idx, branch) in &self.cursor.parents {
            match &blk[*idx] {
                Instr::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    blk = if *branch { then_branch } else { else_branch };
                }
                _ => unreachable!("con tro if khong hop le"),
            }
        }
        blk
    }

    /// Chạy đến điểm chờ tiếp theo, gom sự kiện phát ra.
    fn run(&mut self) -> Result<Vec<VmEvent>, VmError> {
        let mut out: Vec<VmEvent> = Vec::new();
        loop {
            let blk_len = self.current_block().len();
            if self.cursor.ip >= blk_len {
                // Hết block: thoát nhánh if, hoặc return từ call, hoặc kết thúc.
                if let Some((pidx, _)) = self.cursor.parents.pop() {
                    self.cursor.ip = pidx + 1;
                    continue;
                }
                if let Some(ret) = self.calls.pop() {
                    self.cursor = ret;
                    continue;
                }
                self.status = VmStatus::Ended;
                out.push(VmEvent::Ended);
                self.push_snapshot(&out);
                return Ok(out);
            }
            let instr = self.current_block()[self.cursor.ip].clone();
            match instr {
                Instr::Set { effect } => {
                    self.vars.apply(&effect)?;
                    self.cursor.ip += 1;
                }
                Instr::If { cond, .. } => {
                    let taken = self.vars.eval(&cond)?;
                    self.cursor.parents.push((self.cursor.ip, taken));
                    self.cursor.ip = 0;
                }
                Instr::Jump { target } => {
                    let idx = self
                        .story
                        .node_index(&target)
                        .ok_or(VmError::UnknownNode(target.clone()))?;
                    self.cursor = Cursor {
                        node: idx,
                        parents: Vec::new(),
                        ip: 0,
                    };
                    out.push(VmEvent::NodeEntered { node: target });
                }
                Instr::Call { target } => {
                    let idx = self
                        .story
                        .node_index(&target)
                        .ok_or(VmError::UnknownNode(target.clone()))?;
                    let mut ret = self.cursor.clone();
                    ret.ip += 1;
                    self.calls.push(ret);
                    self.cursor = Cursor {
                        node: idx,
                        parents: Vec::new(),
                        ip: 0,
                    };
                    out.push(VmEvent::NodeEntered { node: target });
                }
                Instr::Return => {
                    let ret = self.calls.pop().ok_or(VmError::CallStackUnderflow)?;
                    self.cursor = ret;
                }
                Instr::Say {
                    speaker,
                    text,
                    opts,
                } => {
                    self.cursor.ip += 1;
                    self.status = VmStatus::AwaitAdvance;
                    out.push(VmEvent::Say {
                        speaker,
                        text,
                        opts,
                    });
                    self.push_snapshot(&out);
                    return Ok(out);
                }
                Instr::Wait { ms } => {
                    self.cursor.ip += 1;
                    self.status = VmStatus::AwaitAdvance;
                    out.push(VmEvent::Wait { ms });
                    self.push_snapshot(&out);
                    return Ok(out);
                }
                Instr::Choice { arms } => {
                    self.cursor.ip += 1;
                    let mut visible: Vec<ChoiceArm> = Vec::new();
                    for a in &arms {
                        let ok = match &a.cond {
                            Some(c) => self.vars.eval(c)?,
                            None => true,
                        };
                        if ok {
                            visible.push(a.clone());
                        }
                    }
                    if visible.is_empty() {
                        // Soft-lock: không lựa chọn nào thoả — kết thúc (lint cảnh báo trước).
                        self.status = VmStatus::Ended;
                        out.push(VmEvent::Ended);
                        self.push_snapshot(&out);
                        return Ok(out);
                    }
                    let presented = visible
                        .iter()
                        .enumerate()
                        .map(|(i, a)| PresentedChoice {
                            index: i,
                            text: a.text.clone(),
                        })
                        .collect();
                    self.pending = visible;
                    self.status = VmStatus::AwaitChoice;
                    out.push(VmEvent::Choices { arms: presented });
                    self.push_snapshot(&out);
                    return Ok(out);
                }
                Instr::Show {
                    character,
                    pose,
                    pos,
                } => {
                    out.push(VmEvent::Show {
                        character,
                        pose,
                        pos,
                    });
                    self.cursor.ip += 1;
                }
                Instr::Hide { character } => {
                    out.push(VmEvent::Hide { character });
                    self.cursor.ip += 1;
                }
                Instr::Scene { scene, transition } => {
                    out.push(VmEvent::SceneChanged { scene, transition });
                    self.cursor.ip += 1;
                }
                Instr::Sfx { asset } => {
                    out.push(VmEvent::Sfx { asset });
                    self.cursor.ip += 1;
                }
                Instr::Bgm { asset } => {
                    out.push(VmEvent::Bgm { asset });
                    self.cursor.ip += 1;
                }
                Instr::Ext { command, args } => {
                    out.push(VmEvent::Ext { command, args });
                    self.cursor.ip += 1;
                }
                Instr::End => {
                    self.status = VmStatus::Ended;
                    out.push(VmEvent::Ended);
                    self.push_snapshot(&out);
                    return Ok(out);
                }
            }
        }
    }
}
