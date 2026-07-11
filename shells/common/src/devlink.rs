//! Kênh dev-link WS giữa editor và runtime desktop (spec-devlink.md, M6.2).
//! Chỉ lo giao vận + (de)serialize JSON theo spec-devlink.md. Không tự gọi
//! vào `Runtime` — `State::frame()` poll `inbox`, tự gọi
//! patch_strings/patch_node/patch_story/replay_all, rồi gọi `reply()`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use tungstenite::Message;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatchMsg {
    PatchStrings {
        locale: String,
        entries: BTreeMap<String, String>,
    },
    PatchNode {
        node: serde_json::Value,
        strings: Option<BTreeMap<String, String>>,
    },
    PatchStory {
        story: serde_json::Value,
        strings: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatchReply {
    Ok,
    ReplayStopped { at: usize, reason: String },
    Error { msg: String },
    NodeEntered { node: String },
}

pub struct DevServer {
    pub inbox: Receiver<PatchMsg>,
    outbox: Arc<Mutex<Option<Sender<PatchReply>>>>,
}

impl DevServer {
    pub fn spawn(port: u16) -> std::io::Result<(Self, u16)> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let bound_port = listener.local_addr()?.port();
        let (msg_tx, msg_rx) = channel::<PatchMsg>();
        let outbox: Arc<Mutex<Option<Sender<PatchReply>>>> = Arc::new(Mutex::new(None));
        let outbox_thread = outbox.clone();

        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                handle_connection(stream, msg_tx.clone(), outbox_thread.clone());
            }
        });

        Ok((
            DevServer {
                inbox: msg_rx,
                outbox,
            },
            bound_port,
        ))
    }

    pub fn reply(&self, r: PatchReply) {
        if let Some(tx) = self.outbox.lock().unwrap().as_ref() {
            let _ = tx.send(r);
        }
    }
}

fn handle_connection(
    stream: TcpStream,
    msg_tx: Sender<PatchMsg>,
    outbox: Arc<Mutex<Option<Sender<PatchReply>>>>,
) {
    let Ok(mut ws) = tungstenite::accept(stream) else {
        return;
    };
    let _ = ws
        .get_mut()
        .set_read_timeout(Some(std::time::Duration::from_millis(50)));

    let (reply_tx, reply_rx) = channel::<PatchReply>();
    *outbox.lock().unwrap() = Some(reply_tx);

    loop {
        while let Ok(r) = reply_rx.try_recv() {
            let Ok(text) = serde_json::to_string(&r) else {
                continue;
            };
            if ws.send(Message::Text(text)).is_err() {
                return;
            }
        }
        match ws.read() {
            Ok(Message::Text(t)) => {
                if let Ok(m) = serde_json::from_str::<PatchMsg>(&t) {
                    let _ = msg_tx.send(m);
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => return,
        }
    }
}
