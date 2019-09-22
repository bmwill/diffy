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

#[derive(Debug)]
struct Snake {
    x_start: usize,
    y_start: usize,
    x_end: usize,
    y_end: usize,
}

impl ::std::fmt::Display for Snake {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(
            f,
            "({}, {}) -> ({}, {})",
            self.x_start, self.y_start, self.x_end, self.y_end
        )
    }
}

struct Myers;

impl Myers {
    // The divide part of a divide-and-conquer strategy. A D-path has D+1 snakes some of which may
    // be empty. The divide step requires finding the ceil(D/2) + 1 or middle snake of an optimal
    // D-path. The idea for doing so is to simultaneously run the basic algorithm in both the
    // forward and reverse directions until furthest reaching forward and reverse paths starting at
    // opposing corners 'overlap'.
    fn find_middle_snake(old: &[u8], new: &[u8]) -> (isize, Snake) {
        // In the original paper `n = old.len()` and `m = new.len()`

        // Sum of the length of the sequences being compared
        let max = old.len() + new.len();

        // By Lemma 1 in the paper, the optimal edit script length is odd or even as `delta` is odd
        // or even.
        let delta = old.len() as isize - new.len() as isize;
        let odd = delta & 1 == 1;

        // The array that holds the 'best possible x values' in search from top left to bottom right.
        let mut vf = V::new(max);
        // The array that holds the 'best possible x values' in search from bottom right to top left.
        let mut vb = V::new(max);
        // The initial point at (0, -1)
        vf[1] = 0;
        // The initial point at (N, M+1)
        vb[1] = 0;

        let d_max = ((max + 1) / 2 + 1) as isize;
        // We only need to explore ceil(D/2) + 1
        for d in 0..d_max {
            // Forward path
            println!("forward");
            for k in (-d..=d).step_by(2) {
                println!("d: {} k: {}", d, k);
                let mut x = if k == -d || (k != d && vf[k - 1] < vf[k + 1]) {
                    vf[k + 1]
                } else {
                    vf[k - 1] + 1
                };
                let mut y = (x as isize - k) as usize;

                // The coordinate of the start of a snake
                let (x0, y0) = (x, y);
                //  While these sequences are identical, keep moving through the graph with no cost
                while x < old.len() && y < new.len() && old[x] == new[y] {
                    x += 1;
                    y += 1;
                }

                // This is the new best x value
                vf[k] = x;
                // Only check for connections from the forward search when N - M is odd
                // and when there is a reciprocal k line coming from the other direction.
                if odd && (k - delta).abs() <= (d - 1) {
                    // TODO optimize this so we don't have to compare against old.len()
                    if vf[k] + vb[-(k - delta)] >= old.len() {
                        // Return the snake
                        let snake = Snake {
                            x_start: x0,
                            y_start: y0,
                            x_end: x,
                            y_end: y,
                        };
                        println!("edit distance: {} {}", 2 * d - 1, snake);
                        return (2 * d - 1, snake);
                    }
                }
            }

            // Backward path
            println!("backward");
            for k in (-d..=d).step_by(2) {
                println!("d: {} k: {}", d, k);
                let mut x = if k == -d || (k != d && vb[k - 1] < vb[k + 1]) {
                    vb[k + 1]
                } else {
                    vb[k - 1] + 1
                };
                let mut y = (x as isize - k) as usize;

                // The coordinate of the start of a snake
                let (x0, y0) = (x, y);
                //  While these sequences are identical, keep moving through the graph with no cost
                while x < old.len()
                    && y < new.len()
                    && old[old.len() - x - 1] == new[new.len() - y - 1]
                {
                    x += 1;
                    y += 1;
                }

                // This is the new best x value
                vb[k] = x;

                if !odd && (k - delta).abs() <= d {
                    // TODO optimize this so we don't have to compare against old.len()
                    if vb[k] + vf[-(k - delta)] >= old.len() {
                        // Return the snake
                        let snake = Snake {
                            x_start: old.len() - x,
                            y_start: new.len() - y,
                            x_end: old.len() - x0,
                            y_end: new.len() - y0,
                        };
                        println!("edit distance: {} {}", 2 * d, snake);
                        return (2 * d, snake);
                    }
                }
            }

            // TODO: Maybe there's an opportunity to optimize and bail early?
        }

        unreachable!("unable to find a middle snake");
    }

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

#[cfg(test)]
mod tests {
    use crate::diff::lines;
    use crate::diff::Myers;

    #[test]
    fn diff_test1() {
        let a = b"ABCABBA";
        let b = b"CBABAC";
        Myers::find_middle_snake(&a[..], &b[..]);
    }
}
