#[derive(Debug)]
struct Line<'a> {
    number: usize,
    text: &'a [u8],
}

pub fn diff(a: &[u8], b: &[u8]) {
    let a = lines(a);
    let b = lines(b);

    Myers::gen_diff(Myers::backtrace(Myers::diff(&a, &b), &a, &b), &a, &b);
}

fn lines(a: &[u8]) -> Vec<Line> {
    a.split(|c| *c == b'\n')
        .enumerate()
        .map(|(ln, text)| Line {
            number: ln + 1,
            text,
        })
        .collect()
}

use std::ops::{Index, IndexMut};

#[derive(Debug, Clone)]
struct V {
    max: isize,
    v: Vec<usize>,
}

impl V {
    fn new(max: usize) -> Self {
        Self {
            max: max as isize,
            v: vec![0; 2 * max + 1],
        }
    }
}
impl Index<isize> for V {
    type Output = usize;

    fn index(&self, index: isize) -> &Self::Output {
        &self.v[(index + self.max) as usize]
    }
}

impl IndexMut<isize> for V {
    fn index_mut(&mut self, index: isize) -> &mut Self::Output {
        &mut self.v[(index + self.max) as usize]
    }
}

struct Myers;

impl Myers {
    fn diff(a: &Vec<Line>, b: &Vec<Line>) -> Vec<V> {
        let n = a.len();
        let m = b.len();
        let max = n + m;
        let mut v = V::new(max);
        let mut trace = Vec::new();

        for d in 0..max as isize {
            trace.push(v.clone());

            let mut k = -d;
            while k <= d {
                let mut x = if k == -d || (k != d && v[k - 1] < v[k + 1]) {
                    v[k + 1]
                } else {
                    v[k - 1] + 1
                };
                let mut y = (x as isize - k) as usize;
                //println!("x: {} y: {} k: {} kmapped: {} d: {}", x, y, k, kmapped, d);

                while x < n && y < m && a[x].text == b[y].text {
                    x += 1;
                    y += 1;
                }

                v[k] = x;

                if x >= n && y >= m {
                    return trace;
                }

                k += 2;
            }
        }

        trace
    }

    fn backtrace(trace: Vec<V>, a: &Vec<Line>, b: &Vec<Line>) -> Vec<(usize, usize, usize, usize)> {
        let mut x = a.len();
        let mut y = b.len();
        let mut path = Vec::new();

        for (d, v) in trace.iter().enumerate().rev() {
            let d = d as isize;
            let k = x as isize - y as isize;

            let prev_k = if k == -d || (k != d && v[k - 1] < v[k + 1]) {
                k + 1
            } else {
                k - 1
            };
            let prev_x = v[prev_k];
            let prev_y = (prev_x as isize - prev_k) as usize;

            while x > prev_x && y > prev_y {
                path.push((x - 1, y - 1, x, y));
                //println!("({},{}) -> ({},{})", x - 1, y - 1, x, y);
                x -= 1;
                y -= 1;
            }

            if d > 0 {
                path.push((prev_x, prev_y, x, y));
                //println!("({},{}) -> ({},{})", prev_x, prev_y, x, y);
            }

            x = prev_x;
            y = prev_y;
        }

        path
    }

    fn gen_diff(path: Vec<(usize, usize, usize, usize)>, a: &Vec<Line>, b: &Vec<Line>) {
        let mut diff = Vec::new();

        for &(prev_x, prev_y, x, y) in path.iter().rev() {
            println!("({},{}) -> ({},{})", prev_x, prev_y, x, y);

            if x == prev_x {
                let b_line = &b[prev_y];
                diff.push(Edit::Insertion(b_line));
            } else if y == prev_y {
                let a_line = &a[prev_x];
                diff.push(Edit::Deletion(a_line));
            } else {
                let a_line = &a[prev_x];
                let b_line = &b[prev_y];
                diff.push(Edit::Equal(a_line, b_line));
            }
        }

        // Print Diff
        for edit in diff {
            match edit {
                Edit::Insertion(line) => println!(
                    "\x1b[0;32m+      {: <4} {}\x1b[0m",
                    line.number,
                    String::from_utf8_lossy(line.text)
                ),
                Edit::Deletion(line) => println!(
                    "\x1b[0;31m- {: <4}      {}\x1b[0m",
                    line.number,
                    String::from_utf8_lossy(line.text)
                ),
                Edit::Equal(a, b) => println!(
                    "  {: <4} {: <4} {}",
                    a.number,
                    b.number,
                    String::from_utf8_lossy(b.text)
                ),
            }
        }
    }
}

enum Edit<'b, 'a: 'b> {
    Insertion(&'b Line<'a>),
    Deletion(&'b Line<'a>),
    Equal(&'b Line<'a>, &'b Line<'a>),
}
