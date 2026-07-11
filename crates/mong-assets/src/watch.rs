//! Watcher hot-reload cho dev-mode desktop (mục 3 tài liệu thiết kế).
//! Chỉ báo đường dẫn file đổi qua channel — không tự quyết định "đổi gì thì
//! làm gì" (đó là việc của shell/runtime gọi vào).

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

#[derive(Debug)]
pub struct WatchError(pub String);

pub struct ProjectWatcher {
    // Giữ watcher sống — drop là dừng theo dõi.
    _watcher: RecommendedWatcher,
    pub changes: Receiver<PathBuf>,
}

impl ProjectWatcher {
    pub fn spawn(root: &Path) -> Result<Self, WatchError> {
        let (tx, rx) = channel();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                for p in ev.paths {
                    let _ = tx.send(p);
                }
            }
        })
        .map_err(|e| WatchError(e.to_string()))?;
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| WatchError(e.to_string()))?;
        Ok(ProjectWatcher {
            _watcher: watcher,
            changes: rx,
        })
    }
}
