//! Backend kira: ba sub-track làm ba bus, BGM crossfade bằng tween.

use crate::{AudioError, AudioSink, Bus, CROSSFADE_SECS};
use kira::manager::backend::DefaultBackend;
use kira::manager::{AudioManager, AudioManagerSettings};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::track::{TrackBuilder, TrackHandle};
use kira::tween::Tween;
use std::collections::HashMap;
use std::io::Cursor;
use std::time::Duration;

fn tween(secs: f64) -> Tween {
    Tween {
        duration: Duration::from_secs_f64(secs),
        ..Default::default()
    }
}

/// Lệnh bị hoãn tới lúc `unlock` (web: chờ cử chỉ người dùng đầu tiên).
enum Pending {
    Bgm(Option<String>),
    Sfx(String),
}

pub struct KiraAudio {
    manager: AudioManager<DefaultBackend>,
    tracks: HashMap<Bus, TrackHandle>,
    sounds: HashMap<String, StaticSoundData>,
    /// Bài đang phát: giữ id để phát lại cùng bài không restart.
    current: Option<(String, StaticSoundHandle)>,
    unlocked: bool,
    pending: Vec<Pending>,
}

impl KiraAudio {
    pub fn new() -> Result<Self, AudioError> {
        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|e| AudioError::Backend(e.to_string()))?;
        let mut tracks = HashMap::new();
        for bus in Bus::ALL {
            let t = manager
                .add_sub_track(TrackBuilder::new())
                .map_err(|e| AudioError::Backend(e.to_string()))?;
            tracks.insert(bus, t);
        }
        Ok(KiraAudio {
            manager,
            tracks,
            sounds: HashMap::new(),
            current: None,
            unlocked: false,
            pending: Vec::new(),
        })
    }

    fn play_bgm(&mut self, id: Option<&str>) {
        match id {
            // Cùng bài đang phát: không đụng vào, tránh giật nhạc khi đổi cảnh.
            Some(i) if self.current.as_ref().is_some_and(|(c, _)| c == i) => {}
            Some(i) => {
                let Some(data) = self.sounds.get(i) else {
                    eprintln!("{}", AudioError::UnknownSound(i.into()));
                    return;
                };
                let track = &self.tracks[&Bus::Bgm];
                let data = data
                    .clone()
                    .output_destination(track)
                    .loop_region(..)
                    .fade_in_tween(tween(CROSSFADE_SECS));
                self.fade_out_current();
                match self.manager.play(data) {
                    Ok(h) => self.current = Some((i.to_string(), h)),
                    Err(e) => eprintln!("{}", AudioError::Backend(e.to_string())),
                }
            }
            None => self.fade_out_current(),
        }
    }

    fn fade_out_current(&mut self) {
        if let Some((_, mut h)) = self.current.take() {
            h.stop(tween(CROSSFADE_SECS));
        }
    }

    fn play_sfx(&mut self, id: &str) {
        let Some(data) = self.sounds.get(id) else {
            eprintln!("{}", AudioError::UnknownSound(id.into()));
            return;
        };
        let data = data.clone().output_destination(&self.tracks[&Bus::Sfx]);
        if let Err(e) = self.manager.play(data) {
            eprintln!("{}", AudioError::Backend(e.to_string()));
        }
    }
}

impl AudioSink for KiraAudio {
    fn register(&mut self, id: &str, bytes: Vec<u8>) -> Result<(), AudioError> {
        let data = StaticSoundData::from_cursor(Cursor::new(bytes))
            .map_err(|e| AudioError::Decode(format!("{id}: {e}")))?;
        self.sounds.insert(id.to_string(), data);
        Ok(())
    }

    fn bgm(&mut self, id: Option<&str>) {
        if !self.unlocked {
            self.pending.push(Pending::Bgm(id.map(String::from)));
            return;
        }
        self.play_bgm(id);
    }

    fn sfx(&mut self, id: &str) {
        if !self.unlocked {
            self.pending.push(Pending::Sfx(id.to_string()));
            return;
        }
        self.play_sfx(id);
    }

    fn set_bus_volume(&mut self, bus: Bus, volume: f32) {
        if let Some(t) = self.tracks.get_mut(&bus) {
            t.set_volume(f64::from(volume.clamp(0.0, 1.0)), Tween::default());
        }
    }

    fn unlock(&mut self) {
        if self.unlocked {
            return;
        }
        self.unlocked = true;
        // Chỉ giữ BGM cuối cùng trong hàng đợi: xếp hàng cả một chuỗi đổi
        // nhạc rồi phát tuần tự là vô nghĩa, người chơi chỉ nghe bài cuối.
        let queued = std::mem::take(&mut self.pending);
        let last_bgm = queued.iter().rev().find_map(|p| match p {
            Pending::Bgm(id) => Some(id.clone()),
            _ => None,
        });
        if let Some(id) = last_bgm {
            self.play_bgm(id.as_deref());
        }
        for p in queued {
            if let Pending::Sfx(id) = p {
                self.play_sfx(&id);
            }
        }
    }
}
