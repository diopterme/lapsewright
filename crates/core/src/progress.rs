/// One `progress=` block from `-progress pipe:1`. `None` = not reported yet or `N/A`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Progress {
    pub frame: Option<u64>,
    pub fps: Option<f64>,
    pub out_time_us: Option<i64>,
    pub bitrate_kbps: Option<f64>,
    pub speed: Option<f64>,
    pub is_final: bool,
}

/// Values carry over between blocks, so a key missing from one block keeps its last value.
#[derive(Debug, Default)]
pub struct ProgressParser {
    current: Progress,
}

impl ProgressParser {
    /// Returns an update on each `progress=` line.
    pub fn feed(&mut self, line: &str) -> Option<Progress> {
        let (key, value) = line.split_once('=').filter(|(key, _)| !key.is_empty())?;
        let value = value.trim();
        let current = &mut self.current;
        match key {
            "frame" => current.frame = value.parse().ok(),
            "fps" => current.fps = value.parse().ok(),
            "out_time_us" => current.out_time_us = value.parse().ok(),
            "bitrate" => current.bitrate_kbps = unsigned_decimal(value.strip_suffix("kbits/s")),
            "speed" => current.speed = unsigned_decimal(value.strip_suffix('x')),
            "progress" if value == "continue" || value == "end" => {
                return Some(Progress { is_final: value == "end", ..*current });
            }
            _ => {}
        }
        None
    }
}

/// `[0-9.]+` only, so `-1`, `inf` and `1e3` that `f64::from_str` accepts are rejected.
fn unsigned_decimal(text: Option<&str>) -> Option<f64> {
    text.filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit() || c == '.'))?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(lines: &str) -> Vec<Progress> {
        let mut parser = ProgressParser::default();
        lines.lines().filter_map(|l| parser.feed(l)).collect()
    }

    #[test]
    fn parses_a_block() {
        let updates = feed_all(
            "frame=120\nfps=30.00\nout_time_us=4000000\nbitrate=8000.0kbits/s\nspeed=2.5x\nprogress=continue\n",
        );
        let expected = Progress {
            frame: Some(120),
            fps: Some(30.0),
            out_time_us: Some(4_000_000),
            bitrate_kbps: Some(8000.0),
            speed: Some(2.5),
            is_final: false,
        };
        assert_eq!(updates, [expected]);
    }

    #[test]
    fn end_marks_final() {
        let updates = feed_all("frame=1\nprogress=continue\nframe=2\nprogress=end\n");
        assert_eq!(updates.iter().map(|p| p.is_final).collect::<Vec<_>>(), [false, true]);
    }

    #[test]
    fn missing_keys_keep_previous_values() {
        let updates = feed_all("frame=1\nspeed=1.5x\nprogress=continue\nframe=2\nprogress=continue\n");
        assert_eq!(updates[1].frame, Some(2));
        assert_eq!(updates[1].speed, Some(1.5));
    }

    #[test]
    fn na_and_malformed_values_become_none() {
        let updates = feed_all(
            "frame=5\nspeed=2x\nbitrate=1kbits/s\nfps=1\n\
             frame=N/A\nspeed=N/A\nbitrate=-1kbits/s\nfps=x\nprogress=continue\n",
        );
        assert_eq!(updates[0], Progress::default());
    }

    #[test]
    fn units_must_match_exactly() {
        let updates = feed_all("speed=2.5\nbitrate=8000.0kbit/s\nprogress=continue\n");
        assert_eq!((updates[0].speed, updates[0].bitrate_kbps), (None, None));
    }

    #[test]
    fn splits_at_first_equals_and_trims_value() {
        let updates = feed_all("frame= 7 \nstream_0_0_q=28.0\n=9\nnoise\nprogress = continue\nprogress=continue\n");
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].frame, Some(7));
    }

    #[test]
    fn other_progress_values_emit_nothing() {
        assert!(feed_all("frame=1\nprogress=paused\n").is_empty());
    }
}
