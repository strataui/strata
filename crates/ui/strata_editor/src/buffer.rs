//! Virtualized Balanced B-Tree Rope Line Buffer with O(log N) line indexing and fine-grained mutations.

/// Maximum number of lines per leaf node before splitting.
pub const CHUNK_CAPACITY: usize = 64;
/// Minimum number of lines per leaf node.
pub const MIN_CHUNK_SIZE: usize = 16;
/// Maximum number of child nodes per internal B-Tree node before splitting.
pub const MAX_INTERNAL_CHILDREN: usize = 8;
/// Minimum number of child nodes per internal B-Tree node.
pub const MIN_INTERNAL_CHILDREN: usize = 2;

/// Subtree character, newline, and byte count metrics cached at each node in the B-Tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SubtreeMetrics {
    pub lines: usize,
    pub chars: usize,
    pub bytes: usize,
}

impl SubtreeMetrics {
    pub const fn zero() -> Self {
        Self {
            lines: 0,
            chars: 0,
            bytes: 0,
        }
    }

    pub fn from_lines(lines: &[String]) -> Self {
        let mut chars = 0;
        let mut bytes = 0;
        for l in lines {
            chars += l.chars().count();
            bytes += l.len();
        }
        Self {
            lines: lines.len(),
            chars,
            bytes,
        }
    }

    #[inline]
    pub fn add(&mut self, other: &SubtreeMetrics) {
        self.lines += other.lines;
        self.chars += other.chars;
        self.bytes += other.bytes;
    }
}

/// A leaf node containing a contiguous block of text lines in the B-Tree Rope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RopeLeaf {
    pub lines: Vec<String>,
    pub metrics: SubtreeMetrics,
}

impl RopeLeaf {
    pub fn new(lines: Vec<String>) -> Self {
        let metrics = SubtreeMetrics::from_lines(&lines);
        Self { lines, metrics }
    }

    #[inline]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn recompute_metrics(&mut self) {
        self.metrics = SubtreeMetrics::from_lines(&self.lines);
    }
}

/// Alias for backwards compatibility with tests and callers expecting RopeChunk.
pub type RopeChunk = RopeLeaf;

/// An internal node in the B-Tree Rope maintaining balanced child references and aggregated metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RopeInternal {
    pub children: Vec<RopeNode>,
    pub metrics: SubtreeMetrics,
}

impl RopeInternal {
    pub fn new(children: Vec<RopeNode>) -> Self {
        let mut node = Self {
            children,
            metrics: SubtreeMetrics::zero(),
        };
        node.recompute_metrics();
        node
    }

    #[inline]
    pub fn recompute_metrics(&mut self) {
        let mut m = SubtreeMetrics::zero();
        for child in &self.children {
            m.add(&child.metrics());
        }
        self.metrics = m;
    }
}

/// Result of a B-Tree insertion or split propagation.
pub enum SplitResult {
    NoSplit,
    Split(RopeNode),
}

/// A node in the balanced B-Tree Rope hierarchy (either an internal branch or a leaf node).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RopeNode {
    Leaf(RopeLeaf),
    Internal(RopeInternal),
}

impl RopeNode {
    #[inline]
    pub fn metrics(&self) -> SubtreeMetrics {
        match self {
            RopeNode::Leaf(leaf) => leaf.metrics,
            RopeNode::Internal(internal) => internal.metrics,
        }
    }

    #[inline]
    pub fn line_count(&self) -> usize {
        match self {
            RopeNode::Leaf(leaf) => leaf.metrics.lines,
            RopeNode::Internal(internal) => internal.metrics.lines,
        }
    }

    #[inline]
    pub fn char_count(&self) -> usize {
        match self {
            RopeNode::Leaf(leaf) => leaf.metrics.chars,
            RopeNode::Internal(internal) => internal.metrics.chars,
        }
    }

    #[inline]
    pub fn byte_count(&self) -> usize {
        match self {
            RopeNode::Leaf(leaf) => leaf.metrics.bytes,
            RopeNode::Internal(internal) => internal.metrics.bytes,
        }
    }

    pub fn height(&self) -> usize {
        match self {
            RopeNode::Leaf(_) => 1,
            RopeNode::Internal(internal) => 1 + internal.children.first().map_or(0, |c| c.height()),
        }
    }

    pub fn get_line(&self, mut line_idx: usize) -> Option<&str> {
        match self {
            RopeNode::Leaf(leaf) => leaf.lines.get(line_idx).map(|s| s.as_str()),
            RopeNode::Internal(internal) => {
                for child in &internal.children {
                    let c_lines = child.line_count();
                    if line_idx < c_lines {
                        return child.get_line(line_idx);
                    }
                    line_idx -= c_lines;
                }
                None
            }
        }
    }

    pub fn get_line_mut(&mut self, mut line_idx: usize) -> Option<&mut String> {
        match self {
            RopeNode::Leaf(leaf) => leaf.lines.get_mut(line_idx),
            RopeNode::Internal(internal) => {
                for child in &mut internal.children {
                    let c_lines = child.line_count();
                    if line_idx < c_lines {
                        return child.get_line_mut(line_idx);
                    }
                    line_idx -= c_lines;
                }
                None
            }
        }
    }

    pub fn insert_char(&mut self, mut line_idx: usize, col: usize, ch: char) {
        match self {
            RopeNode::Leaf(leaf) => {
                if let Some(l) = leaf.lines.get_mut(line_idx) {
                    let byte_idx = char_to_byte(l, col);
                    l.insert(byte_idx, ch);
                    leaf.metrics.chars += 1;
                    leaf.metrics.bytes += ch.len_utf8();
                }
            }
            RopeNode::Internal(internal) => {
                for child in &mut internal.children {
                    let c_lines = child.line_count();
                    if line_idx < c_lines {
                        child.insert_char(line_idx, col, ch);
                        internal.metrics.chars += 1;
                        internal.metrics.bytes += ch.len_utf8();
                        return;
                    }
                    line_idx -= c_lines;
                }
            }
        }
    }

    pub fn insert_str_single_line(&mut self, mut line_idx: usize, col: usize, text: &str) {
        match self {
            RopeNode::Leaf(leaf) => {
                if let Some(l) = leaf.lines.get_mut(line_idx) {
                    let byte_idx = char_to_byte(l, col);
                    l.insert_str(byte_idx, text);
                    leaf.metrics.chars += text.chars().count();
                    leaf.metrics.bytes += text.len();
                }
            }
            RopeNode::Internal(internal) => {
                let text_chars = text.chars().count();
                let text_bytes = text.len();
                for child in &mut internal.children {
                    let c_lines = child.line_count();
                    if line_idx < c_lines {
                        child.insert_str_single_line(line_idx, col, text);
                        internal.metrics.chars += text_chars;
                        internal.metrics.bytes += text_bytes;
                        return;
                    }
                    line_idx -= c_lines;
                }
            }
        }
    }

    pub fn delete_char_at(&mut self, mut line_idx: usize, col: usize) -> Option<usize> {
        match self {
            RopeNode::Leaf(leaf) => {
                let l = leaf.lines.get_mut(line_idx)?;
                let byte_offset = char_to_byte(l, col);
                if byte_offset > 0 {
                    let prev_char = l[..byte_offset].chars().next_back()?;
                    let prev_byte_offset = byte_offset - prev_char.len_utf8();
                    let byte_len = prev_char.len_utf8();
                    l.replace_range(prev_byte_offset..byte_offset, "");
                    leaf.metrics.chars = leaf.metrics.chars.saturating_sub(1);
                    leaf.metrics.bytes = leaf.metrics.bytes.saturating_sub(byte_len);
                    Some(byte_len)
                } else {
                    None
                }
            }
            RopeNode::Internal(internal) => {
                for child in &mut internal.children {
                    let c_lines = child.line_count();
                    if line_idx < c_lines {
                        if let Some(byte_len) = child.delete_char_at(line_idx, col) {
                            internal.metrics.chars = internal.metrics.chars.saturating_sub(1);
                            internal.metrics.bytes =
                                internal.metrics.bytes.saturating_sub(byte_len);
                            return Some(byte_len);
                        }
                        return None;
                    }
                    line_idx -= c_lines;
                }
                None
            }
        }
    }

    pub fn split_line(&mut self, mut line_idx: usize, col: usize) -> SplitResult {
        match self {
            RopeNode::Leaf(leaf) => {
                if let Some(left) = leaf.lines.get_mut(line_idx) {
                    let byte_idx = char_to_byte(left, col);
                    let right = left[byte_idx..].to_string();
                    left.truncate(byte_idx);
                    leaf.lines.insert(line_idx + 1, right);
                    leaf.metrics.lines += 1;
                    if leaf.lines.len() > CHUNK_CAPACITY {
                        let mid = leaf.lines.len() / 2;
                        let right_lines = leaf.lines.split_off(mid);
                        leaf.recompute_metrics();
                        SplitResult::Split(RopeNode::Leaf(RopeLeaf::new(right_lines)))
                    } else {
                        SplitResult::NoSplit
                    }
                } else {
                    SplitResult::NoSplit
                }
            }
            RopeNode::Internal(internal) => {
                let num_children = internal.children.len();
                let mut target_child_idx = num_children.saturating_sub(1);
                for (i, child) in internal.children.iter().enumerate() {
                    let c_lines = child.line_count();
                    if line_idx < c_lines || i == num_children - 1 {
                        target_child_idx = i;
                        break;
                    }
                    line_idx -= c_lines;
                }

                let split = internal.children[target_child_idx].split_line(line_idx, col);
                if let SplitResult::Split(new_sibling) = split {
                    internal.children.insert(target_child_idx + 1, new_sibling);
                }
                internal.recompute_metrics();

                if internal.children.len() > MAX_INTERNAL_CHILDREN {
                    let mid = internal.children.len() / 2;
                    let right_children = internal.children.split_off(mid);
                    let right_internal = RopeInternal::new(right_children);
                    internal.recompute_metrics();
                    SplitResult::Split(RopeNode::Internal(right_internal))
                } else {
                    SplitResult::NoSplit
                }
            }
        }
    }

    pub fn append_to_line(&mut self, mut line_idx: usize, text: &str) {
        match self {
            RopeNode::Leaf(leaf) => {
                if let Some(l) = leaf.lines.get_mut(line_idx) {
                    l.push_str(text);
                    leaf.metrics.chars += text.chars().count();
                    leaf.metrics.bytes += text.len();
                }
            }
            RopeNode::Internal(internal) => {
                let text_chars = text.chars().count();
                let text_bytes = text.len();
                for child in &mut internal.children {
                    let c_lines = child.line_count();
                    if line_idx < c_lines {
                        child.append_to_line(line_idx, text);
                        internal.metrics.chars += text_chars;
                        internal.metrics.bytes += text_bytes;
                        return;
                    }
                    line_idx -= c_lines;
                }
            }
        }
    }

    pub fn insert_line(&mut self, mut line_idx: usize, text: String) -> SplitResult {
        match self {
            RopeNode::Leaf(leaf) => {
                let insert_pos = line_idx.min(leaf.lines.len());
                let text_chars = text.chars().count();
                let text_bytes = text.len();
                leaf.lines.insert(insert_pos, text);
                leaf.metrics.lines += 1;
                leaf.metrics.chars += text_chars;
                leaf.metrics.bytes += text_bytes;
                if leaf.lines.len() > CHUNK_CAPACITY {
                    let mid = leaf.lines.len() / 2;
                    let right_lines = leaf.lines.split_off(mid);
                    leaf.recompute_metrics();
                    SplitResult::Split(RopeNode::Leaf(RopeLeaf::new(right_lines)))
                } else {
                    SplitResult::NoSplit
                }
            }
            RopeNode::Internal(internal) => {
                let num_children = internal.children.len();
                let mut target_child_idx = num_children.saturating_sub(1);

                for (i, child) in internal.children.iter().enumerate() {
                    let c_lines = child.line_count();
                    if line_idx <= c_lines || i == num_children - 1 {
                        target_child_idx = i;
                        break;
                    }
                    line_idx -= c_lines;
                }

                let split = internal.children[target_child_idx].insert_line(line_idx, text);
                if let SplitResult::Split(new_sibling) = split {
                    internal.children.insert(target_child_idx + 1, new_sibling);
                }
                internal.recompute_metrics();

                if internal.children.len() > MAX_INTERNAL_CHILDREN {
                    let mid = internal.children.len() / 2;
                    let right_children = internal.children.split_off(mid);
                    let right_internal = RopeInternal::new(right_children);
                    internal.recompute_metrics();
                    SplitResult::Split(RopeNode::Internal(right_internal))
                } else {
                    SplitResult::NoSplit
                }
            }
        }
    }

    pub fn delete_line(&mut self, mut line_idx: usize) -> Option<String> {
        match self {
            RopeNode::Leaf(leaf) => {
                if line_idx < leaf.lines.len() {
                    let removed = leaf.lines.remove(line_idx);
                    leaf.metrics.lines -= 1;
                    leaf.metrics.chars = leaf.metrics.chars.saturating_sub(removed.chars().count());
                    leaf.metrics.bytes = leaf.metrics.bytes.saturating_sub(removed.len());
                    Some(removed)
                } else {
                    None
                }
            }
            RopeNode::Internal(internal) => {
                let mut target = None;
                for (i, child) in internal.children.iter_mut().enumerate() {
                    let c_lines = child.line_count();
                    if line_idx < c_lines {
                        target = Some((i, child.delete_line(line_idx)));
                        break;
                    }
                    line_idx -= c_lines;
                }

                if let Some((idx, deleted)) = target {
                    if internal.children[idx].line_count() == 0 && internal.children.len() > 1 {
                        internal.children.remove(idx);
                    }
                    internal.recompute_metrics();
                    deleted
                } else {
                    None
                }
            }
        }
    }

    pub fn slice_lines<'a>(
        &'a self,
        line_offset: usize,
        start: usize,
        end: usize,
        out: &mut Vec<(usize, &'a str)>,
    ) {
        let node_lines = self.line_count();
        let node_start = line_offset;
        let node_end = line_offset + node_lines;

        if node_end <= start || node_start >= end {
            return;
        }

        match self {
            RopeNode::Leaf(leaf) => {
                for (i, line) in leaf.lines.iter().enumerate() {
                    let abs_line = node_start + i;
                    if abs_line >= start && abs_line < end {
                        out.push((abs_line, line.as_str()));
                    }
                }
            }
            RopeNode::Internal(internal) => {
                let mut curr = line_offset;
                for child in &internal.children {
                    let child_lines = child.line_count();
                    child.slice_lines(curr, start, end, out);
                    curr += child_lines;
                }
            }
        }
    }

    /// Mutates the line at `line_idx` in place and bubbles exact `(chars, bytes)` metric deltas
    /// up the B-Tree spine in O(log N), keeping `SubtreeMetrics` strictly invariant.
    pub fn mutate_line<R>(
        &mut self,
        mut line_idx: usize,
        f: impl FnOnce(&mut String) -> R,
    ) -> Option<(R, isize, isize)> {
        match self {
            RopeNode::Leaf(leaf) => {
                let line = leaf.lines.get_mut(line_idx)?;
                let old_chars = line.chars().count() as isize;
                let old_bytes = line.len() as isize;
                let res = f(line);
                let new_chars = line.chars().count() as isize;
                let new_bytes = line.len() as isize;
                let d_chars = new_chars - old_chars;
                let d_bytes = new_bytes - old_bytes;
                leaf.metrics.chars = (leaf.metrics.chars as isize + d_chars).max(0) as usize;
                leaf.metrics.bytes = (leaf.metrics.bytes as isize + d_bytes).max(0) as usize;
                Some((res, d_chars, d_bytes))
            }
            RopeNode::Internal(internal) => {
                for child in &mut internal.children {
                    let c_lines = child.line_count();
                    if line_idx < c_lines {
                        let (res, d_chars, d_bytes) = child.mutate_line(line_idx, f)?;
                        internal.metrics.chars =
                            (internal.metrics.chars as isize + d_chars).max(0) as usize;
                        internal.metrics.bytes =
                            (internal.metrics.bytes as isize + d_bytes).max(0) as usize;
                        return Some((res, d_chars, d_bytes));
                    }
                    line_idx -= c_lines;
                }
                None
            }
        }
    }

    /// Computes the exact byte offset of `(line_idx, char_col)` in the `\n`-joined document
    /// in O(log N) time using cached `SubtreeMetrics`.
    pub fn position_to_byte(&self, mut line_idx: usize, char_col: usize) -> usize {
        match self {
            RopeNode::Leaf(leaf) => {
                let mut offset = 0usize;
                for (i, line) in leaf.lines.iter().enumerate() {
                    if i == line_idx {
                        return offset + char_to_byte(line, char_col);
                    }
                    offset += line.len() + 1;
                }
                // Clamped past end of leaf
                offset.saturating_sub(1)
            }
            RopeNode::Internal(internal) => {
                let mut offset = 0usize;
                for child in &internal.children {
                    let m = child.metrics();
                    if line_idx < m.lines {
                        return offset + child.position_to_byte(line_idx, char_col);
                    }
                    offset += m.bytes + m.lines;
                    line_idx -= m.lines;
                }
                offset.saturating_sub(1)
            }
        }
    }

    /// Converts a byte offset in the `\n`-joined document into `(line_idx, char_col)` in O(log N).
    pub fn byte_to_position(&self, mut byte_offset: usize, line_base: usize) -> (usize, usize) {
        match self {
            RopeNode::Leaf(leaf) => {
                for (i, line) in leaf.lines.iter().enumerate() {
                    let line_bytes = line.len();
                    if byte_offset <= line_bytes {
                        return (line_base + i, byte_to_char(line, byte_offset));
                    }
                    // +1 accounts for the `\n` separator after this line
                    byte_offset = byte_offset.saturating_sub(line_bytes + 1);
                }
                let last_idx = leaf.lines.len().saturating_sub(1);
                let last_chars = leaf.lines.get(last_idx).map_or(0, |l| l.chars().count());
                (line_base + last_idx, last_chars)
            }
            RopeNode::Internal(internal) => {
                let mut cur_line = line_base;
                let last_child_idx = internal.children.len().saturating_sub(1);
                for (i, child) in internal.children.iter().enumerate() {
                    let m = child.metrics();
                    let span_with_newlines = m.bytes + m.lines;
                    if byte_offset < span_with_newlines || i == last_child_idx {
                        return child.byte_to_position(byte_offset, cur_line);
                    }
                    byte_offset -= span_with_newlines;
                    cur_line += m.lines;
                }
                (cur_line, 0)
            }
        }
    }

    pub fn write_text(&self, out: &mut String, is_first: &mut bool) {
        match self {
            RopeNode::Leaf(leaf) => {
                for line in &leaf.lines {
                    if !*is_first {
                        out.push('\n');
                    }
                    out.push_str(line);
                    *is_first = false;
                }
            }
            RopeNode::Internal(internal) => {
                for child in &internal.children {
                    child.write_text(out, is_first);
                }
            }
        }
    }
}

/// Converts a character-based column index to a byte offset within `s`.
pub fn char_to_byte(s: &str, char_idx: usize) -> usize {
    let mut byte_offset = 0;
    for (i, ch) in s.chars().enumerate() {
        if i == char_idx {
            return byte_offset;
        }
        byte_offset += ch.len_utf8();
    }
    s.len()
}

/// Converts a byte offset within `s` to a character-based column index.
pub fn byte_to_char(s: &str, byte_idx: usize) -> usize {
    let clamped_byte = byte_idx.min(s.len());
    let mut char_count = 0;
    let mut curr_byte = 0;
    for ch in s.chars() {
        if curr_byte >= clamped_byte {
            break;
        }
        curr_byte += ch.len_utf8();
        char_count += 1;
    }
    char_count
}

/// Balanced B-Tree Rope text data structure for logarithmic-time
/// line indexing, splitting, joining, and editing across massive documents (100,000+ lines).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRope {
    root: RopeNode,
}

impl Default for TextRope {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRope {
    /// Creates an empty rope containing a single empty line.
    pub fn new() -> Self {
        Self {
            root: RopeNode::Leaf(RopeLeaf::new(vec![String::new()])),
        }
    }

    /// Accesses the root node of the B-Tree.
    pub fn root(&self) -> &RopeNode {
        &self.root
    }

    /// Accesses the root node of the B-Tree mutably.
    pub fn root_mut(&mut self) -> &mut RopeNode {
        &mut self.root
    }

    /// Returns the tree height of the balanced B-Tree rope.
    pub fn height(&self) -> usize {
        self.root.height()
    }

    /// Constructs a TextRope by splitting an input string along linebreaks (`\n` or `\r\n`).
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Self {
        let raw_lines: Vec<String> = text
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect();
        let lines = if raw_lines.is_empty() {
            vec![String::new()]
        } else {
            raw_lines
        };

        let mut current_level: Vec<RopeNode> = lines
            .chunks(CHUNK_CAPACITY / 2)
            .map(|chunk| RopeNode::Leaf(RopeLeaf::new(chunk.to_vec())))
            .collect();

        if current_level.is_empty() {
            current_level.push(RopeNode::Leaf(RopeLeaf::new(vec![String::new()])));
        }

        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in current_level.chunks(MAX_INTERNAL_CHILDREN) {
                next_level.push(RopeNode::Internal(RopeInternal::new(chunk.to_vec())));
            }
            current_level = next_level;
        }

        Self {
            root: current_level.remove(0),
        }
    }

    /// Total number of lines in the buffer.
    #[inline]
    pub fn line_count(&self) -> usize {
        self.root.line_count()
    }

    /// Total number of characters in the buffer.
    #[inline]
    pub fn total_char_count(&self) -> usize {
        self.root.char_count()
    }

    /// Total number of bytes in the buffer.
    #[inline]
    pub fn total_byte_count(&self) -> usize {
        self.root.byte_count()
    }

    /// Inserts a new line into the B-Tree at index `line_idx`, maintaining B-Tree balance.
    pub fn insert_line(&mut self, line_idx: usize, text: String) {
        if let SplitResult::Split(sibling) = self.root.insert_line(line_idx, text) {
            let old_root =
                std::mem::replace(&mut self.root, RopeNode::Leaf(RopeLeaf::new(Vec::new())));
            self.root = RopeNode::Internal(RopeInternal::new(vec![old_root, sibling]));
        }
    }

    /// Deletes the line at index `line_idx`, collapsing internal nodes if necessary.
    pub fn delete_line(&mut self, line_idx: usize) -> Option<String> {
        let deleted = self.root.delete_line(line_idx);
        if let RopeNode::Internal(ref mut internal) = self.root {
            if internal.children.len() == 1 {
                self.root = internal.children.remove(0);
            }
        }
        deleted
    }

    /// Returns a reference to the line at the specified 0-based index.
    pub fn line(&self, idx: usize) -> Option<&str> {
        self.root.get_line(idx)
    }

    /// Returns a mutable reference to the line at the specified 0-based index.
    pub fn line_mut(&mut self, idx: usize) -> Option<&mut String> {
        self.root.get_line_mut(idx)
    }

    /// Returns the character length of the line at the specified index.
    pub fn line_len(&self, idx: usize) -> usize {
        self.line(idx).map_or(0, |l| l.chars().count())
    }

    /// Returns the character count of the line at the specified index.
    pub fn line_char_count(&self, idx: usize) -> usize {
        self.line_len(idx)
    }

    /// Returns the byte length of the line at the specified index.
    pub fn line_byte_len(&self, idx: usize) -> usize {
        self.line(idx).map_or(0, |l| l.len())
    }

    /// Inserts a character at the specified line and character column.
    pub fn insert_char(&mut self, line: usize, col: usize, ch: char) {
        if ch == '\n' {
            self.split_line(line, col);
            return;
        }

        self.root.insert_char(line, col, ch);
    }

    /// Inserts a string at the specified line and character column, automatically handling multiline breaks.
    pub fn insert_str(&mut self, line: usize, col: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        if !text.contains('\n') {
            self.root.insert_str_single_line(line, col, text);
            return;
        }

        let parts: Vec<&str> = text.split('\n').collect();
        let remainder = match self.root.mutate_line(line, |l| {
            let byte_idx = char_to_byte(l, col);
            let rem = l[byte_idx..].to_string();
            l.truncate(byte_idx);
            l.push_str(parts[0]);
            rem
        }) {
            Some((rem, _, _)) => rem,
            None => return,
        };

        let mut insert_idx = line + 1;
        for part in &parts[1..parts.len() - 1] {
            self.insert_line(insert_idx, part.to_string());
            insert_idx += 1;
        }

        let mut last_line = parts[parts.len() - 1].to_string();
        last_line.push_str(&remainder);
        self.insert_line(insert_idx, last_line);
    }

    /// Splits a line into two lines at the given character column index.
    pub fn split_line(&mut self, line: usize, col: usize) {
        if let SplitResult::Split(sibling) = self.root.split_line(line, col) {
            let old_root =
                std::mem::replace(&mut self.root, RopeNode::Leaf(RopeLeaf::new(Vec::new())));
            self.root = RopeNode::Internal(RopeInternal::new(vec![old_root, sibling]));
        }
    }

    /// Joins the specified line with the subsequent line.
    pub fn join_line_with_next(&mut self, line: usize) {
        if line + 1 < self.line_count() {
            if let Some(next_line) = self.delete_line(line + 1) {
                self.root.append_to_line(line, &next_line);
            }
        }
    }

    /// Deletes a character before the specified position (backspace).
    pub fn delete_backwards(&mut self, line: usize, col: usize) -> Option<(usize, usize)> {
        if col > 0 {
            if self.root.delete_char_at(line, col).is_some() {
                return Some((line, col - 1));
            }
        } else if line > 0 {
            let prev_line_len = self.line_len(line - 1);
            self.join_line_with_next(line - 1);
            return Some((line - 1, prev_line_len));
        }
        None
    }

    /// Deletes text within the given bounding character range and returns the deleted substring.
    pub fn delete_range(
        &mut self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        if start_line > end_line || (start_line == end_line && start_col >= end_col) {
            return String::new();
        }

        if start_line == end_line {
            return self
                .root
                .mutate_line(start_line, |line| {
                    let sc = char_to_byte(line, start_col);
                    let ec = char_to_byte(line, end_col);
                    let deleted = line[sc..ec].to_string();
                    line.replace_range(sc..ec, "");
                    deleted
                })
                .map(|(d, _, _)| d)
                .unwrap_or_default();
        }

        let mut deleted = String::new();

        // 1. First line
        let remainder = if let Some((first_tail, _, _)) = self.root.mutate_line(start_line, |first| {
            let sc = char_to_byte(first, start_col);
            let tail = first[sc..].to_string();
            first.truncate(sc);
            tail
        }) {
            deleted.push_str(&first_tail);
            deleted.push('\n');
            true
        } else {
            false
        };

        // 2. Middle lines
        let remove_count = end_line - start_line - 1;
        for _ in 0..remove_count {
            if start_line + 1 < self.line_count() {
                if let Some(mid) = self.delete_line(start_line + 1) {
                    deleted.push_str(&mid);
                    deleted.push('\n');
                }
            }
        }

        // 3. Last line
        if remainder && start_line + 1 < self.line_count() {
            if let Some(last) = self.delete_line(start_line + 1) {
                let ec = char_to_byte(&last, end_col);
                deleted.push_str(&last[..ec]);
                let tail = &last[ec..];
                if !tail.is_empty() {
                    self.root.append_to_line(start_line, tail);
                }
            }
        }

        deleted
    }

    /// Total UTF-8 byte length of the serialized `\n`-joined document.
    #[inline]
    pub fn total_text_byte_len(&self) -> usize {
        self.root.byte_count() + self.line_count().saturating_sub(1)
    }

    /// Computes the exact byte offset of `(line, char_col)` in the `\n`-joined document in O(log N).
    pub fn position_to_byte(&self, line: usize, char_col: usize) -> usize {
        let clamped_line = line.min(self.line_count().saturating_sub(1));
        self.root.position_to_byte(clamped_line, char_col)
    }

    /// Computes the Tree-Sitter `Point { row, column }` (byte column within row) for `(line, char_col)`.
    pub fn position_to_point(&self, line: usize, char_col: usize) -> tree_sitter::Point {
        let clamped_line = line.min(self.line_count().saturating_sub(1));
        let byte_col = self
            .line(clamped_line)
            .map_or(0, |l| char_to_byte(l, char_col));
        tree_sitter::Point {
            row: clamped_line,
            column: byte_col,
        }
    }

    /// Converts a byte offset in the `\n`-joined document into `(line, char_col)` in O(log N).
    pub fn byte_to_position(&self, byte_offset: usize) -> (usize, usize) {
        self.root.byte_to_position(byte_offset, 0)
    }

    /// Computes an exact [`tree_sitter::InputEdit`] describing replacing `[start..old_end)` with
    /// `inserted_text` **before** applying the mutation to the rope.
    pub fn compute_input_edit(
        &self,
        start: crate::cursor::Position,
        old_end: crate::cursor::Position,
        inserted_text: &str,
    ) -> (tree_sitter::InputEdit, crate::cursor::Position) {
        let start_byte = self.position_to_byte(start.line, start.col);
        let old_end_byte = self.position_to_byte(old_end.line, old_end.col);
        let new_end_byte = start_byte + inserted_text.len();

        let start_position = self.position_to_point(start.line, start.col);
        let old_end_position = self.position_to_point(old_end.line, old_end.col);

        let (new_end_position, new_end_char_pos) = if let Some(last_nl) = inserted_text.rfind('\n') {
            let added_rows = inserted_text.bytes().filter(|&b| b == b'\n').count();
            let tail = &inserted_text[last_nl + 1..];
            (
                tree_sitter::Point {
                    row: start_position.row + added_rows,
                    column: tail.len(),
                },
                crate::cursor::Position::new(start.line + added_rows, tail.chars().count()),
            )
        } else {
            (
                tree_sitter::Point {
                    row: start_position.row,
                    column: start_position.column + inserted_text.len(),
                },
                crate::cursor::Position::new(start.line, start.col + inserted_text.chars().count()),
            )
        };

        (
            tree_sitter::InputEdit {
                start_byte,
                old_end_byte,
                new_end_byte,
                start_position,
                old_end_position,
                new_end_position,
            },
            new_end_char_pos,
        )
    }

    /// Returns a slice of lines for virtual viewport rendering in logarithmic time.
    pub fn slice_lines(&self, start: usize, count: usize) -> Vec<(usize, &str)> {
        let total = self.line_count();
        let end = (start + count).min(total);
        if start >= end {
            return Vec::new();
        }

        let mut out = Vec::with_capacity(end - start);
        self.root.slice_lines(0, start, end, &mut out);
        out
    }

    /// Serializes entire rope back into a single newline-separated String.
    pub fn to_text(&self) -> String {
        let mut out = String::with_capacity(self.root.byte_count() + self.line_count());
        let mut is_first = true;
        self.root.write_text(&mut out, &mut is_first);
        out
    }
}

/// Fast, indexed line-based text buffer backed by a balanced B-Tree Rope.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LineBuffer {
    rope: TextRope,
}

impl LineBuffer {
    /// Creates an empty buffer containing a single empty line.
    pub fn new() -> Self {
        Self {
            rope: TextRope::new(),
        }
    }

    /// Constructs a LineBuffer by splitting an input string along linebreaks (`\n` or `\r\n`).
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Self {
        Self {
            rope: TextRope::from_str(text),
        }
    }

    /// Accesses the underlying balanced B-Tree TextRope.
    pub fn rope(&self) -> &TextRope {
        &self.rope
    }

    /// Accesses the underlying balanced B-Tree TextRope mutably.
    pub fn rope_mut(&mut self) -> &mut TextRope {
        &mut self.rope
    }

    /// Total number of lines in the buffer.
    pub fn line_count(&self) -> usize {
        self.rope.line_count()
    }

    /// Total number of characters in the buffer.
    pub fn total_char_count(&self) -> usize {
        self.rope.total_char_count()
    }

    /// Total number of bytes in the buffer (excluding `\n` separators).
    pub fn total_byte_count(&self) -> usize {
        self.rope.total_byte_count()
    }

    /// Total UTF-8 byte length of the serialized `\n`-joined document.
    pub fn total_text_byte_len(&self) -> usize {
        self.rope.total_text_byte_len()
    }

    /// Computes the exact byte offset of `pos` in the `\n`-joined document in O(log N).
    pub fn position_to_byte(&self, pos: crate::cursor::Position) -> usize {
        self.rope.position_to_byte(pos.line, pos.col)
    }

    /// Computes the Tree-Sitter `Point` (byte column) for `pos`.
    pub fn position_to_point(&self, pos: crate::cursor::Position) -> tree_sitter::Point {
        self.rope.position_to_point(pos.line, pos.col)
    }

    /// Converts a byte offset in the `\n`-joined document into a character `Position` in O(log N).
    pub fn byte_to_position(&self, byte_offset: usize) -> crate::cursor::Position {
        let (line, col) = self.rope.byte_to_position(byte_offset);
        crate::cursor::Position::new(line, col)
    }

    /// Computes an exact [`tree_sitter::InputEdit`] describing replacing `[start..old_end)` with
    /// `inserted_text` **before** applying the mutation to the buffer.
    pub fn compute_input_edit(
        &self,
        start: crate::cursor::Position,
        old_end: crate::cursor::Position,
        inserted_text: &str,
    ) -> (tree_sitter::InputEdit, crate::cursor::Position) {
        self.rope.compute_input_edit(start, old_end, inserted_text)
    }

    /// Tree height of the B-Tree rope.
    pub fn height(&self) -> usize {
        self.rope.height()
    }

    /// Returns a reference to the line at the specified 0-based index.
    pub fn line(&self, idx: usize) -> Option<&str> {
        self.rope.line(idx)
    }

    /// Returns a mutable reference to the line at the specified 0-based index.
    pub fn line_mut(&mut self, idx: usize) -> Option<&mut String> {
        self.rope.line_mut(idx)
    }

    /// Returns the character length of the line at the specified index.
    pub fn line_len(&self, idx: usize) -> usize {
        self.rope.line_len(idx)
    }

    /// Returns the character count of the line at the specified index.
    pub fn line_char_count(&self, idx: usize) -> usize {
        self.rope.line_char_count(idx)
    }

    /// Returns the byte length of the line at the specified index.
    pub fn line_byte_len(&self, idx: usize) -> usize {
        self.rope.line_byte_len(idx)
    }

    /// Inserts a character at the specified line and character column.
    pub fn insert_char(&mut self, line: usize, col: usize, ch: char) {
        self.rope.insert_char(line, col, ch);
    }

    /// Inserts a string at the specified line and character column, automatically handling multiline breaks.
    pub fn insert_str(&mut self, line: usize, col: usize, text: &str) {
        self.rope.insert_str(line, col, text);
    }

    /// Splits a line into two lines at the given character column index.
    pub fn split_line(&mut self, line: usize, col: usize) {
        self.rope.split_line(line, col);
    }

    /// Joins the specified line with the subsequent line.
    pub fn join_line_with_next(&mut self, line: usize) {
        self.rope.join_line_with_next(line);
    }

    /// Deletes a character before the specified position (backspace).
    pub fn delete_backwards(&mut self, line: usize, col: usize) -> Option<(usize, usize)> {
        self.rope.delete_backwards(line, col)
    }

    /// Deletes text within the given bounding character range and returns the deleted substring.
    pub fn delete_range(
        &mut self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        self.rope
            .delete_range(start_line, start_col, end_line, end_col)
    }

    /// Returns a slice of lines for virtual viewport rendering in logarithmic time.
    pub fn slice_lines(&self, start: usize, count: usize) -> Vec<(usize, &str)> {
        self.rope.slice_lines(start, count)
    }

    /// Serializes entire buffer back into a single newline-separated String.
    pub fn to_text(&self) -> String {
        self.rope.to_text()
    }
}

impl std::str::FromStr for LineBuffer {
    type Err = std::convert::Infallible;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_str(text))
    }
}

impl std::fmt::Display for LineBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_text())
    }
}
