//! mong-plugin — skeleton, trien khai o moc M5 theo docs/thiet-ke-mong-engine.md.

/// Trang thai crate, dung de kiem tra workspace noi dung.
pub fn crate_status() -> &'static str {
    "skeleton (M5)"
}

#[cfg(test)]
mod tests {
    #[test]
    fn co_mat_trong_workspace() {
        assert!(super::crate_status().contains("skeleton"));
    }
}
