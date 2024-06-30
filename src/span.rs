use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineColumn {
    /// 1-based line
    pub line: usize,
    /// 0-based column
    pub column: usize,
}

impl Default for LineColumn {
    fn default() -> Self {
        Self { line: 1, column: 0 }
    }
}

impl Ord for LineColumn {
    fn cmp(&self, other: &Self) -> Ordering {
        let ordering = self.line.cmp(&other.line);
        if ordering == Ordering::Equal {
            self.column.cmp(&other.column)
        } else {
            ordering
        }
    }
}

impl PartialOrd for LineColumn {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Span {
    start: LineColumn,
    end: LineColumn,
}

impl Span {
    pub fn new(start: LineColumn, end: LineColumn) -> Self {
        Self { start, end }
    }

    pub fn start(&self) -> LineColumn {
        self.start
    }

    pub fn end(&self) -> LineColumn {
        self.end
    }
}
