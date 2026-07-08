//! Fuzz-lite cho read_pack: bytes cắt cụt / lật bit / rác thuần → Err là
//! hành vi đúng, panic là bug.

use mong_assets::{read_pack, write_pack, EntryKind, PackEntry};
use std::panic::{catch_unwind, AssertUnwindSafe};

struct TestRng(u64); // nhân bản SplitMix64 như fuzz_lite.rs của mong-core

impl TestRng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

#[test]
fn read_pack_khong_panic_voi_bytes_hong() {
    let entries = vec![PackEntry {
        name: "story.ir".into(),
        kind: EntryKind::StoryIr,
        data: br#"{"gia":"tri"}"#.to_vec(),
    }];
    let mut good = Vec::new();
    write_pack(&mut good, &entries).unwrap();

    let cases = std::env::var("MONG_FUZZ_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500u64);
    for i in 0..cases {
        let seed = 0x7ACC ^ i; // đổi thành hằng hợp lệ, ví dụ 0x7ACC
        let good = good.clone();
        let ok = catch_unwind(AssertUnwindSafe(move || {
            let mut r = TestRng(seed);
            let mut bytes = good;
            match r.below(3) {
                0 => bytes.truncate(r.below(bytes.len() as u64 + 1) as usize),
                1 => {
                    for _ in 0..=r.below(8) {
                        let idx = r.below(bytes.len() as u64) as usize;
                        bytes[idx] ^= r.next() as u8;
                    }
                }
                _ => bytes = (0..r.below(200)).map(|_| r.next() as u8).collect(),
            }
            let _ = read_pack(&mut &bytes[..]);
        }))
        .is_ok();
        assert!(ok, "read_pack panic — tai lap voi seed = {seed}");
    }
}
