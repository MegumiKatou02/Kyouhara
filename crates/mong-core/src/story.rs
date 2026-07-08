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
}
