//! Cấu trúc dữ liệu cốt truyện đã biên dịch (đầu vào của máy ảo).

use crate::ir::{Instr, NodeId, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Một phân đoạn: dãy lệnh IR chạy tuần tự.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub scene: Option<String>,
    pub body: Vec<Instr>,
}

/// Toàn bộ cốt truyện. Đây là nội dung entry `story.ir` trong .mongpack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Story {
    pub format_version: u32,
    #[serde(default)]
    pub title: String,
    pub default_locale: String,
    #[serde(default)]
    pub locales: Vec<String>,
    /// Giá trị khởi tạo của biến — BTreeMap để thứ tự luôn xác định.
    #[serde(default)]
    pub variables: BTreeMap<String, Value>,
    pub start: NodeId,
    pub nodes: Vec<Node>,
}

impl Story {
    /// Tìm chỉ số node theo id.
    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// Thay node cùng id (hot reload dev — spec-devlink). Trả về `false` nếu
    /// story không có node đó: thêm node mới không đi đường này mà đi
    /// `patch_story`. Chỉ dùng qua Runtime::patch_node — VM đang chạy không
    /// bao giờ bị mutate story giữa chừng, replay dựng VM mới.
    pub fn replace_node(&mut self, node: Node) -> bool {
        match self.nodes.iter_mut().find(|n| n.id == node.id) {
            Some(slot) => {
                *slot = node;
                true
            }
            None => false,
        }
    }

    /// Hash nội dung cốt truyện (FNV-1a 64 trên JSON canonical).
    /// BTreeMap + thứ tự field cố định → cùng Story luôn cho cùng hash,
    /// trên mọi nền tảng. Save file dùng nó để phát hiện cốt truyện đã đổi.
    pub fn hash64(&self) -> u64 {
        let bytes = serde_json::to_vec(self).expect("story luon serialize duoc");
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }
}
