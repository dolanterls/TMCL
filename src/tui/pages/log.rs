use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct LogBuffer {
    lines: VecDeque<String>,
    max_lines: usize,
    offset: usize,
    follow: bool,
}

impl LogBuffer {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            max_lines: max_lines.max(1),
            offset: 0,
            follow: true,
        }
    }

    pub fn append(&mut self, line: impl Into<String>) {
        self.lines.push_back(line.into());
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.offset = 0;
    }

    pub fn scroll(&mut self, delta: isize, max_offset: usize) {
        self.offset = self.offset.min(max_offset);
        if delta.is_negative() {
            self.follow = false;
            self.offset = self
                .offset
                .saturating_add(delta.unsigned_abs())
                .min(max_offset);
        } else {
            self.offset = self.offset.saturating_sub(delta as usize);
            if self.offset == 0 {
                self.follow = true;
            }
        }
    }

    pub fn set_follow(&mut self, follow: bool) {
        self.follow = follow;
        if follow {
            self.offset = 0;
        }
    }

    pub fn toggle_follow(&mut self) {
        self.set_follow(!self.follow);
    }

    pub fn follow(&self) -> bool {
        self.follow
    }

    pub fn scroll_offset(&self) -> usize {
        self.offset
    }

    pub fn search(&self, query: &str) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|line| line.contains(query))
            .map(String::as_str)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn matching_lines(&self, query: &str) -> Vec<String> {
        self.lines
            .iter()
            .filter(|line| query.is_empty() || line.contains(query))
            .cloned()
            .collect()
    }

    pub fn contains_query(&self, query: &str) -> bool {
        query.is_empty() || self.lines.iter().any(|line| line.contains(query))
    }
}

#[cfg(test)]
mod tests {
    use super::LogBuffer;

    #[test]
    fn keeps_latest_lines_and_supports_search_and_scroll() {
        let mut buffer = LogBuffer::new(2);
        buffer.append("first");
        buffer.append("second");
        buffer.append("third");
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.search("third"), vec!["third"]);
        buffer.scroll(-10, 1);
        assert_eq!(buffer.scroll_offset(), 1);
        buffer.scroll(10, 1);
        assert_eq!(buffer.scroll_offset(), 0);
        buffer.clear();
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn scroll_up_moves_to_older_rows_and_scroll_down_returns_to_latest() {
        let mut buffer = LogBuffer::new(10);
        for index in 0..5 {
            buffer.append(format!("line {index}"));
        }

        buffer.scroll(-1, 4);
        assert_eq!(buffer.scroll_offset(), 1);
        buffer.scroll(-10, 2);
        assert_eq!(buffer.scroll_offset(), 2);
        buffer.scroll(1, 2);
        assert_eq!(buffer.scroll_offset(), 1);
        buffer.scroll(10, 2);
        assert_eq!(buffer.scroll_offset(), 0);
        assert!(buffer.follow());
    }

    #[test]
    fn manual_scroll_pauses_follow_and_returning_to_bottom_resumes_it() {
        let mut buffer = LogBuffer::new(10);
        for index in 0..5 {
            buffer.append(format!("line {index}"));
        }
        assert!(buffer.follow());
        buffer.scroll(-1, 4);
        assert!(!buffer.follow());
        buffer.scroll(10, 4);
        assert!(buffer.follow());
        buffer.set_follow(false);
        assert!(!buffer.follow());
        buffer.toggle_follow();
        assert!(buffer.follow());
    }
}
