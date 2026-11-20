//! Shaped runs kept from one paint to the next, found by where they are and, when a row
//! scrolled, by what they show.
//!
//! Kept by row alone, every row missed when output scrolled the screen by one line, and
//! each non-ASCII run (Chinese, box drawing, bullets) went back through CoreText: in a real
//! window with an agent printing, `shape_cell` was 5% of the main thread. A run that only
//! moved is shaped the same, so a miss by position looks for it among the last paint's.
use super::Run;
use rustc_hash::FxHashMap;

/// What shaping a run depends on besides the font and metrics the whole cache is kept for:
/// its column doesn't change the glyphs, so a run that moved sideways is found too.
#[derive(PartialEq, Eq, Hash)]
struct Shape {
    text: String,
    cols: u16,
    fg: u32,
    bold: bool,
    italic: bool,
    faint: bool,
}
impl Shape {
    fn of(run: &Run) -> Self {
        Self {
            text: run.text.clone(),
            cols: run.cols,
            fg: run.fg,
            bold: run.attrs.bold,
            italic: run.attrs.italic,
            faint: run.attrs.faint,
        }
    }
}

/// Each row's shaped runs in paint order. Holds only what the viewport shows: the last
/// paint's runs, never a history of a long-running terminal.
pub struct Rows<V> {
    rows: Vec<Vec<(Run, V)>>,
    /// The runs as they were when this paint first missed, by content. Built at most once
    /// a paint, and only in a paint that missed.
    moved: Option<FxHashMap<Shape, V>>,
}
impl<V> Default for Rows<V> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            moved: None,
        }
    }
}
impl<V: Clone> Rows<V> {
    pub fn clear(&mut self) {
        self.rows.clear();
    }
    /// Align retained shapes with their likely new viewport row. A run still
    /// has to match in `get` before any shape is reused.
    pub fn scroll(&mut self, from: usize, to: usize) {
        let moved = from.abs_diff(to);
        if moved != 0 && moved < self.rows.len() {
            if to > from {
                self.rows.rotate_left(moved);
            } else {
                self.rows.rotate_right(moved);
            }
        }
    }
    /// Before a paint of `rows` rows.
    pub fn begin(&mut self, rows: usize) {
        self.moved = None;
        self.rows.resize_with(rows, Vec::new);
    }
    /// The shaped form of the `index`th shaped run of row `y`: the one kept there when it
    /// is the same run, else the same run from anywhere in the last paint, else `shape`.
    pub fn get(&mut self, y: usize, index: usize, run: &Run, shape: impl FnOnce() -> V) -> &V {
        let row = &self.rows[y];
        if row.get(index).is_none_or(|(old, _)| old != run) {
            let rows = &self.rows;
            let moved = self.moved.get_or_insert_with(|| {
                rows.iter()
                    .flatten()
                    .map(|(run, v)| (Shape::of(run), v.clone()))
                    .collect()
            });
            let value = moved.get(&Shape::of(run)).cloned().unwrap_or_else(shape);
            let row = &mut self.rows[y];
            let entry = (run.clone(), value);
            if index < row.len() {
                row[index] = entry;
            } else {
                row.push(entry);
            }
        }
        &self.rows[y][index].1
    }
    /// After row `y` is painted: runs past the `used` it painted are gone.
    pub fn end_row(&mut self, y: usize, used: usize) {
        self.rows[y].truncate(used);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::screen::Attrs;

    fn run(col: u16, text: &str) -> Run {
        Run {
            col,
            cols: text.chars().count() as u16 * 2,
            text: text.into(),
            fg: 0xffffff,
            bg: None,
            attrs: Attrs::default(),
        }
    }

    /// Paints `screen` and returns how many runs had to be shaped.
    fn paint(cache: &mut Rows<String>, screen: &[Vec<Run>]) -> usize {
        let mut shaped = 0;
        cache.begin(screen.len());
        for (y, row) in screen.iter().enumerate() {
            for (i, r) in row.iter().enumerate() {
                let got = cache.get(y, i, r, || {
                    shaped += 1;
                    r.text.clone()
                });
                assert_eq!(got, &r.text);
            }
            cache.end_row(y, row.len());
        }
        shaped
    }

    #[test]
    fn scrolled_rows_are_not_shaped_again() {
        let lines: Vec<Vec<Run>> = (0..30)
            .map(|i| vec![run(0, &format!("第{i}行")), run(20, "──")])
            .collect();
        let mut cache = Rows::default();
        assert_eq!(paint(&mut cache, &lines[0..24]), 48);
        assert_eq!(paint(&mut cache, &lines[0..24]), 0, "nothing changed");
        // Output scrolls the screen up a line, then three more at once.
        assert_eq!(paint(&mut cache, &lines[1..25]), 1, "only the new line");
        assert_eq!(paint(&mut cache, &lines[4..28]), 3);
        // A run that moved sideways keeps its glyphs.
        let mut shifted = lines[4..28].to_vec();
        shifted[0][0].col = 6;
        assert_eq!(paint(&mut cache, &shifted), 0);
    }

    #[test]
    fn aligning_viewport_reuses_shifted_rows_in_place() {
        let rows: Vec<_> = ["第一行", "第二行", "第三行"]
            .map(|text| vec![run(0, text)])
            .into();
        let mut cache = Rows::default();
        assert_eq!(paint(&mut cache, &rows), 3);

        cache.scroll(10, 11);
        cache.begin(3);
        for (y, row) in rows[1..].iter().enumerate() {
            let got = cache.get(y, 0, &row[0], || {
                panic!("a shifted run should still be cached")
            });
            assert_eq!(got, &row[0].text);
        }
        assert!(cache.moved.is_none());

        cache.scroll(11, 10);
        cache.begin(3);
        assert_eq!(cache.get(0, 0, &rows[0][0], || unreachable!()), "第一行");
        assert!(cache.moved.is_none());
    }

    #[test]
    fn a_run_that_looks_different_is_shaped_again() {
        let mut cache = Rows::default();
        paint(&mut cache, &[vec![run(0, "中文")]]);
        let mut bold = run(0, "中文");
        bold.attrs.bold = true;
        assert_eq!(paint(&mut cache, &[vec![bold]]), 1);
        let mut red = run(0, "中文");
        red.fg = 0xff0000;
        assert_eq!(paint(&mut cache, &[vec![red]]), 1);
    }

    #[test]
    fn it_keeps_only_what_the_last_paint_showed() {
        let mut cache = Rows::default();
        paint(&mut cache, &[vec![run(0, "甲")], vec![run(0, "乙")]]);
        paint(&mut cache, &[vec![run(0, "丙")]]);
        assert_eq!(cache.rows.iter().flatten().count(), 1);
        assert_eq!(
            paint(&mut cache, &[vec![run(0, "甲")]]),
            1,
            "gone two paints later"
        );
    }
}
